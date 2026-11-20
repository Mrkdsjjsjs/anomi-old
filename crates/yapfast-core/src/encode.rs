//! Encoding for Go Live and the camera, through native/vp8.c with the libvpx
//! inside libwebrtc, native/h264.c with x264 (vendor/x264-src), and
//! native/hwenc.c with FFmpeg's hardware encoders (vendor/ffmpeg-src) on the
//! graphics adapter asked for ([`Device`]). Frames arrive as RGBA or I420
//! and leave as VP8 frames,
//! H264 or H265 access units in Annex B form, or AV1 temporal units of OBUs
//! that give their sizes.

use std::{
    ffi::{c_int, c_long, c_longlong, c_uchar, c_uint, c_ulong},
    path::PathBuf,
};

use anyhow::{Result, bail};
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};
use yapfast_discord::rtp::Codec;
// FFmpeg's and x264's libraries, which native/hwenc.c and native/h264.c
// call: linked only while named.
#[cfg(hwenc)]
use ffmpeg_src as _;
use x264_src as _;

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

type Encode = unsafe extern "C" fn(
    *mut Raw,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    c_longlong,
    c_ulong,
    c_int,
    *mut c_uchar,
    usize,
    *mut c_int,
) -> c_long;
type SetBitrate = unsafe extern "C" fn(*mut Raw, c_uint) -> c_int;
type Free = unsafe extern "C" fn(*mut Raw);

unsafe extern "C" {
    fn yapfast_vp8_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_vp8_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        out: *mut c_uchar,
        capacity: usize,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_vp8_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_vp8_free(encoder: *mut Raw);
    fn yapfast_h264_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
        effort: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_h264_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        out: *mut c_uchar,
        capacity: usize,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_h264_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_h264_free(encoder: *mut Raw);
}

#[cfg(hwenc)]
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_hwenc_open(
        codec: c_uint,
        on: c_uint,
        node: *const std::ffi::c_char,
        kernel: *const std::ffi::c_char,
        gpu: c_int,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        report: *mut std::ffi::c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_hwenc_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        out: *mut c_uchar,
        capacity: usize,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_hwenc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_free(encoder: *mut Raw);
}

/// RTP's video clock.
pub const CLOCK: u64 = 90_000;

/// How much work the H264 encoder puts into each frame: x264's superfast
/// preset, or its faster one, for a sharper picture at about twice the CPU.
/// VP8 takes no effort.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    #[default]
    Light,
    High,
}

/// Where an encoder runs: on the processor, or on a graphics adapter's
/// encoder, which `stream_worker` chooses by Settings' "Encode streams on".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Device {
    /// libvpx for VP8, x264 for H264.
    Processor,
    /// The first graphics encoder that opens, of whichever adapter: NVENC,
    /// then AMF, then VA-API on each render node.
    AnyGpu,
    /// NVIDIA's NVENC on the CUDA device numbered `index`, CUDA counting
    /// NVIDIA's cards in PCI bus order, as `CUDA_DEVICE_ORDER=PCI_BUS_ID`
    /// has it: the stream worker starts with that.
    Nvenc { index: u32 },
    /// AMD's AMF, on the adapter it picks (Windows).
    Amf,
    /// VA-API on the render node `node`, with the libva drivers its kernel
    /// driver `kernel` calls for (radeonsi for amdgpu); an empty `kernel`
    /// reads it from sysfs.
    Vaapi { node: PathBuf, kernel: String },
}

impl Device {
    pub fn is_gpu(&self) -> bool {
        *self != Self::Processor
    }
}

pub struct Encoder {
    raw: *mut Raw,
    codec: Codec,
    effort: Effort,
    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc".
    engine: String,
    /// Where it runs: the processor once x264 took over from a graphics
    /// encoder that failed.
    device: Device,
    encode: Encode,
    set_bitrate: SetBitrate,
    free: Free,
    /// The bitrate aimed for, in kbit/s.
    kbps: u32,
    /// The kbit/s it was opened to hold ([`Self::holding`]): the level it
    /// signals takes the stream up to that. 0 when none was asked.
    held: u32,
    width: u32,
    height: u32,
    fps: u32,
    /// The threads x264 or libvpx slices each frame over, as many as
    /// [`crate::cpu::threads`] gave as it opened; 0 for a graphics encoder.
    threads: u32,
    /// The frame as I420, for [`Self::encode`]: a full-size luma plane, then
    /// two quarter-size chroma planes. Empty until it's first needed: the
    /// screen and the camera arrive as I420.
    planes: Vec<u8>,
    out: Vec<u8>,
}

// SAFETY: no library keeps thread-local state past a call; one thread uses
// the encoder at a time.
unsafe impl Send for Encoder {}

/// One encoded frame.
pub struct Frame<'a> {
    pub data: &'a [u8],
    pub keyframe: bool,
}

/// An opened encoder's handle and functions, its name, and the threads it
/// slices each frame over: 0 for a graphics encoder.
type Opened = (*mut Raw, Encode, SetBitrate, Free, String, u32);

impl Encoder {
    /// An encoder for `width` by `height` frames, which must be even. With
    /// `hardware`, H264 goes to the first GPU encoder that opens, and to
    /// x264 otherwise. H265 and AV1 have only the GPU's.
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
    ) -> Result<Self> {
        if hardware
            && let Ok(encoder) = Self::on(codec, width, height, fps, kbps, effort, &Device::AnyGpu)
        {
            return Ok(encoder);
        }
        Self::on(codec, width, height, fps, kbps, effort, &Device::Processor)
    }

    /// An encoder for `width` by `height` frames, which must be even, on
    /// `device` and nowhere else: the error says why its encoder didn't open.
    pub fn on(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        device: &Device,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let opened = match device {
            Device::Processor => open_software(codec, width, height, fps, kbps, effort)?,
            gpu => open_hardware(codec, gpu, width, height, fps, kbps)?,
        };
        let (raw, encode, set_bitrate, free, engine, threads) = opened;
        let pixels = (width * height) as usize;
        Ok(Self {
            raw,
            codec,
            effort,
            engine,
            device: device.clone(),
            encode,
            set_bitrate,
            free,
            kbps,
            held: 0,
            width,
            height,
            fps,
            threads,
            planes: Vec::new(),
            // Even a keyframe of noise fits, at any bitrate: an encoder
            // codes a block that won't shrink as its pixels.
            out: vec![0; pixels * 2 + 4096],
        })
    }

    /// An encoder as [`Self::on`] opens it, but at `held` kbit/s, aiming
    /// for `kbps` from its first frame. Each encoder chooses the level its
    /// stream signals by the bitrate it opens at (x264 and FFmpeg's VA-API
    /// encoders by their VBV's rate and buffer, NVENC's own choice by the
    /// bitrate too), and keeps it: a stream whose target later rises to a
    /// ceiling past 9 Mbit/s would pass that level's bitrate. With `held`
    /// 0 it's [`Self::on`].
    #[allow(clippy::too_many_arguments)]
    pub fn holding(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        held: u32,
        effort: Effort,
        device: &Device,
    ) -> Result<Self> {
        let mut encoder = Self::on(codec, width, height, fps, kbps.max(held), effort, device)?;
        encoder.held = held;
        if held > kbps {
            encoder.set_bitrate(kbps)?;
        }
        Ok(encoder)
    }

    /// The kbit/s the level it signals was chosen for, at least, when it
    /// was opened to hold them ([`Self::holding`]); 0 otherwise.
    pub fn held(&self) -> u32 {
        self.held
    }

    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc".
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// Where it runs.
    pub fn device(&self) -> &Device {
        &self.device
    }

    fn hardware(&self) -> bool {
        self.device.is_gpu()
    }

    /// Puts `opened`, on the processor, in place of the encoder running now.
    fn replace(&mut self, opened: Opened) {
        self.device = Device::Processor;
        // SAFETY: raw came from its codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
        (
            self.raw,
            self.encode,
            self.set_bitrate,
            self.free,
            self.engine,
            self.threads,
        ) = opened;
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Whether it encodes as `fps` frames a second at `effort` would: each
    /// encoder paces its rate control by the frame rate it opened with, but
    /// only x264 works harder for `Effort::High`.
    pub fn tuned(&self, fps: u32, effort: Effort) -> bool {
        self.fps == fps && (self.effort == effort || self.engine != "x264")
    }

    /// Whether its threads fit `threads`, as [`crate::cpu::threads`] counts
    /// them for the CPUs the process runs on now: x264 and libvpx slice
    /// each frame over as many as they opened with, so other cores need
    /// another encoder; a graphics encoder fits any.
    pub fn fits_threads(&self, threads: usize) -> bool {
        self.threads == 0 || self.threads as usize == threads
    }

    /// The bitrate aimed for, in kbit/s.
    pub fn bitrate(&self) -> u32 {
        self.kbps
    }

    /// Aims for `kbps` from the next frame on, without a keyframe.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw came from this codec's constructor and is still live.
        if unsafe { (self.set_bitrate)(self.raw, kbps) } != 0 {
            bail!("the {:?} encoder refused {kbps} kbit/s", self.codec);
        }
        self.kbps = kbps;
        Ok(())
    }

    /// Encodes RGBA pixels of the encoder's size, shown at `pts` in
    /// [`CLOCK`] units. None when the encoder skipped the frame.
    pub fn encode(&mut self, rgba: &[u8], pts: u64, keyframe: bool) -> Result<Option<Frame<'_>>> {
        let (width, height) = (self.width, self.height);
        let pixels = (width * height) as usize;
        if rgba.len() < pixels * 4 {
            bail!("a frame smaller than the encoder's");
        }
        let mut planes = std::mem::take(&mut self.planes);
        planes.resize(pixels + pixels / 2, 0);
        let (y, chroma) = planes.split_at_mut(pixels);
        let (u, v) = chroma.split_at_mut(pixels / 4);
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory.
        yuv_helper::abgr_to_i420(
            rgba,
            width * 4,
            y,
            width,
            u,
            width / 2,
            v,
            width / 2,
            width as i32,
            height as i32,
        );
        let encoded =
            self.encode_planes([(y, width), (u, width / 2), (v, width / 2)], pts, keyframe);
        self.planes = planes;
        self.frame(encoded?)
    }

    /// Encodes an I420 picture of the encoder's size, like [`Self::encode`].
    pub fn encode_i420(
        &mut self,
        picture: &I420Buffer,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame<'_>>> {
        if (picture.width(), picture.height()) != (self.width, self.height) {
            bail!("a picture of another size than the encoder's");
        }
        let (y, u, v) = picture.data();
        let (y_stride, u_stride, v_stride) = picture.strides();
        let encoded =
            self.encode_planes([(y, y_stride), (u, u_stride), (v, v_stride)], pts, keyframe);
        self.frame(encoded?)
    }

    /// Encodes a camera's picture, at least the encoder's size: its top left
    /// corner, when it's larger, as a picture an odd size loses a row or a
    /// column.
    pub fn encode_picture(
        &mut self,
        picture: &crate::picture::I420<'_>,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame<'_>>> {
        if picture.width < self.width || picture.height < self.height {
            bail!("a picture smaller than the encoder's");
        }
        let [y_stride, u_stride, v_stride] = picture.strides;
        let encoded = self.encode_planes(
            [
                (picture.y, y_stride),
                (picture.u, u_stride),
                (picture.v, v_stride),
            ],
            pts,
            keyframe,
        );
        self.frame(encoded?)
    }

    /// Encodes three planes, each with its stride, into `out`: the encoded
    /// size and whether it's a keyframe, or None when the frame was skipped.
    /// When a hardware H264 encoder fails, x264 takes over from this frame,
    /// with a keyframe; H265 and AV1 have nothing to take over.
    fn encode_planes(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<(usize, bool)>> {
        match self.encode_once(planes, pts, keyframe) {
            Err(error) if self.hardware() && self.codec == Codec::H264 => {
                tracing::warn!(engine = self.engine, %error, "the GPU's encoder failed; encoding with x264");
                let (codec, width, height, fps) = (self.codec, self.width, self.height, self.fps);
                // At the bitrate the level was chosen for, as it opened.
                let kbps = self.kbps;
                let at = kbps.max(self.held);
                let opened = open_software(codec, width, height, fps, at, self.effort)?;
                self.replace(opened);
                if at > kbps {
                    self.set_bitrate(kbps)?;
                }
                self.encode_once(planes, pts, true)
            }
            encoded => encoded,
        }
    }

    fn encode_once(
        &mut self,
        [(y, y_stride), (u, u_stride), (v, v_stride)]: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<(usize, bool)>> {
        let mut is_key: c_int = 0;
        // SAFETY: the planes hold a picture of this encoder's size at these
        // strides, and the output buffer is sized for its frames; all stay
        // put during the call.
        let written = unsafe {
            (self.encode)(
                self.raw,
                y.as_ptr(),
                y_stride as c_int,
                u.as_ptr(),
                u_stride as c_int,
                v.as_ptr(),
                v_stride as c_int,
                pts as c_longlong,
                (CLOCK / u64::from(self.fps)) as c_ulong,
                c_int::from(keyframe),
                self.out.as_mut_ptr(),
                self.out.len(),
                &mut is_key,
            )
        };
        match written {
            ..0 => bail!("{:?} encoding failed ({written})", self.codec),
            0 => Ok(None),
            _ => Ok(Some((written as usize, is_key != 0))),
        }
    }

    fn frame(&self, encoded: Option<(usize, bool)>) -> Result<Option<Frame<'_>>> {
        Ok(encoded.map(|(size, keyframe)| Frame {
            data: &self.out[..size],
            keyframe,
        }))
    }
}

/// libvpx for VP8, x264 for H264.
fn open_software(
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    effort: Effort,
) -> Result<Opened> {
    // From the CPUs the process runs on, the cores Settings chose: never
    // the machine's, and on Windows, whose available_parallelism doesn't
    // see CPU sets, the mask applied.
    let threads = u32::try_from(crate::cpu::threads())
        .unwrap_or(4)
        .clamp(1, 4);
    // SAFETY: plain values; null means the library refused the configuration.
    let opened: Opened = match codec {
        Codec::Vp8 => (
            unsafe { yapfast_vp8_new(width, height, fps, kbps, threads) },
            yapfast_vp8_encode,
            yapfast_vp8_set_bitrate,
            yapfast_vp8_free,
            "libvpx".into(),
            threads,
        ),
        Codec::H264 => (
            unsafe { yapfast_h264_new(width, height, fps, kbps, threads, effort as c_uint) },
            yapfast_h264_encode,
            yapfast_h264_set_bitrate,
            yapfast_h264_free,
            "x264".into(),
            threads,
        ),
        Codec::H265 | Codec::Av1 => bail!(
            "{} encodes only on a GPU, and this one has no encoder for it",
            codec.name()
        ),
    };
    if opened.0.is_null() {
        bail!("could not start the {codec:?} encoder");
    }
    Ok(opened)
}

/// The encoder for `codec` on the graphics adapter `device` names, or why
/// none opened there.
#[cfg(hwenc)]
fn open_hardware(
    codec: Codec,
    device: &Device,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
) -> Result<Opened> {
    use std::ffi::CString;
    let number = match codec {
        Codec::H264 => 0,
        Codec::H265 => 1,
        Codec::Av1 => 2,
        Codec::Vp8 => bail!("VP8 encodes only on the processor"),
    };
    // As native/hwenc.c numbers them.
    let (on, gpu, node, kernel) = match device {
        Device::Processor => bail!("the processor has no graphics encoder"),
        Device::AnyGpu => (0, -1, None, None),
        Device::Nvenc { index } => (1, c_int::try_from(*index)?, None, None),
        Device::Amf => (2, -1, None, None),
        Device::Vaapi { node, kernel } => {
            let node = CString::new(node.as_os_str().as_encoded_bytes())?;
            (3, -1, Some(node), Some(CString::new(kernel.as_str())?))
        }
    };
    let pointer =
        |text: &Option<CString>| text.as_ref().map_or(std::ptr::null(), |text| text.as_ptr());
    let mut report = [0u8; 512];
    // SAFETY: plain values, strings that end with a zero and outlive the
    // call, and a buffer of the size given, which the shim ends with a zero.
    let raw = unsafe {
        yapfast_hwenc_open(
            number,
            on,
            pointer(&node),
            pointer(&kernel),
            gpu,
            width,
            height,
            fps,
            kbps,
            report.as_mut_ptr().cast(),
            report.len(),
        )
    };
    let report = std::ffi::CStr::from_bytes_until_nul(&report).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    );
    if raw.is_null() {
        tracing::info!(?codec, ?device, tried = report, "no GPU encoder opened");
        bail!("no {} encoder opened on {device:?}: {report}", codec.name());
    }
    tracing::debug!(engine = report, ?codec, "encoding on the GPU");
    Ok((
        raw,
        yapfast_hwenc_encode,
        yapfast_hwenc_set_bitrate,
        yapfast_hwenc_free,
        report,
        0,
    ))
}

#[cfg(not(hwenc))]
fn open_hardware(codec: Codec, _: &Device, _: u32, _: u32, _: u32, _: u32) -> Result<Opened> {
    bail!("this build has no graphics encoder for {}", codec.name())
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from this codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;

    /// Four frames of moving colour, the third a forced keyframe, and the
    /// engine that encoded them.
    fn encode_four(
        codec: Codec,
        effort: Effort,
        (width, height): (u32, u32),
        hardware: bool,
    ) -> (Vec<(bool, Vec<u8>)>, String) {
        let mut encoder = Encoder::new(codec, width, height, 30, 500, effort, hardware).unwrap();
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut frames = vec![];
        for frame in 0..4u64 {
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
            if frame == 1 {
                encoder.set_bitrate(250).unwrap();
                assert_eq!(encoder.bitrate(), 250);
            }
            let encoded = encoder
                .encode(&rgba, frame * 3000, frame == 2)
                .unwrap()
                .unwrap();
            frames.push((encoded.keyframe, encoded.data.to_vec()));
        }
        assert!(Encoder::new(codec, 63, 36, 30, 500, effort, hardware).is_err());
        (frames, encoder.engine().to_owned())
    }

    /// A new frame rate needs another encoder; a new effort only x264's.
    #[test]
    fn knows_when_a_new_quality_needs_another_encoder() {
        let x264 = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
        assert!(x264.tuned(30, Effort::Light));
        assert!(!x264.tuned(30, Effort::High));
        assert!(!x264.tuned(60, Effort::Light));
        let vp8 = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, false).unwrap();
        assert!(vp8.tuned(30, Effort::High), "VP8 takes no effort");
        assert!(!vp8.tuned(15, Effort::Light));
    }

    /// An encoder opened to hold 50 Mbit/s, the unlock's most, aims for its
    /// start from the first frame, says what it holds, and rises to it
    /// without opening again. One opened to hold less than it aims for
    /// opens at its aim; one asked to hold nothing is a plain one.
    #[test]
    fn holds_a_higher_ceiling_than_it_aims_for() {
        let at = |held| {
            Encoder::holding(
                Codec::H264,
                64,
                36,
                30,
                2_500,
                held,
                Effort::Light,
                &Device::Processor,
            )
            .unwrap()
        };
        let mut x264 = at(50_000);
        assert_eq!((x264.bitrate(), x264.held()), (2_500, 50_000));
        let rgba = vec![90u8; 64 * 36 * 4];
        let first = x264.encode(&rgba, 0, true).unwrap().unwrap();
        assert!(first.keyframe);
        x264.set_bitrate(50_000).unwrap();
        assert_eq!((x264.bitrate(), x264.held()), (50_000, 50_000));
        assert!(x264.encode(&rgba, 3000, false).unwrap().is_some());
        let below = at(2_000);
        assert_eq!((below.bitrate(), below.held()), (2_500, 2_000));
        let plain = at(0);
        assert_eq!((plain.bitrate(), plain.held()), (2_500, 0));
    }

    /// On the processor, only the processor's encoders; on an adapter, only
    /// that adapter's, failing rather than falling back to x264, so the
    /// stream worker can try the next one and say so. Neither touches a GPU.
    #[test]
    fn opens_only_where_it_is_told() {
        let x264 = Encoder::on(
            Codec::H264,
            64,
            36,
            30,
            500,
            Effort::Light,
            &Device::Processor,
        )
        .unwrap();
        assert_eq!((x264.engine(), x264.device()), ("x264", &Device::Processor));
        assert!(
            Encoder::on(
                Codec::H265,
                64,
                36,
                30,
                500,
                Effort::Light,
                &Device::Processor
            )
            .is_err()
        );
        // NVIDIA's kernel driver has no VA-API encoder to try.
        let nowhere = Device::Vaapi {
            node: PathBuf::from("/dev/dri/by-path/pci-0000:ff:00.0-render"),
            kernel: "nvidia".into(),
        };
        let error = Encoder::on(Codec::H264, 64, 36, 30, 500, Effort::Light, &nowhere)
            .err()
            .expect("nothing to open");
        eprintln!("{error:#}");
        assert!(Encoder::on(Codec::Vp8, 64, 36, 30, 500, Effort::Light, &Device::AnyGpu).is_err());
        assert!(!Device::Processor.is_gpu() && nowhere.is_gpu());
    }

    /// x264 slices each frame over a thread for each logical CPU the
    /// process runs on, up to 4 (`cpu::threads`), which in a stream worker
    /// are the cores Settings chose; once they give another count it needs
    /// another encoder.
    #[test]
    fn x264_takes_its_threads_from_the_cpus_it_runs_on() {
        let x264 = Encoder::on(
            Codec::H264,
            64,
            36,
            30,
            500,
            Effort::Light,
            &Device::Processor,
        )
        .unwrap();
        let threads = crate::cpu::threads();
        assert_eq!(x264.threads as usize, threads);
        assert!(x264.fits_threads(threads));
        assert!(!x264.fits_threads(threads % 4 + 1), "other cores");
    }

    #[test]
    fn encodes_vp8_keyframes_and_deltas() {
        let (frames, _) = encode_four(Codec::Vp8, Effort::Light, (64, 36), false);
        for (keyframe, data) in &frames {
            // A keyframe's tag has bit 0 clear, and its size follows the start code.
            assert_eq!(*keyframe, data[0] & 1 == 0);
            if *keyframe {
                assert_eq!(&data[3..6], &[0x9d, 0x01, 0x2a]);
                let size = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]) & 0x3fff;
                assert_eq!((size(6), size(8)), (64, 36));
            }
        }
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// Main profile H264 at `size` that decodes: parameter sets and an IDR
    /// first, a forced IDR third. Returns the engine.
    fn check_h264(effort: Effort, size: (u32, u32), hardware: bool) -> String {
        let (frames, engine) = encode_four(Codec::H264, effort, size, hardware);
        {
            let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
            assert_eq!(keys[..3], [true, false, true]);
            let units = |data| yapfast_discord::rtp::nal_units(data);
            let types =
                |data| -> Vec<u8> { units(data).iter().map(|unit| unit[0] & 0x1f).collect() };
            let first = types(&frames[0].1);
            assert!(
                first.starts_with(&[7, 8]) && first.contains(&5),
                "parameter sets, then an IDR slice: {first:?}"
            );
            // The sequence parameter set's profile_idc: Main, as Discord's
            // client sends.
            assert_eq!(units(&frames[0].1)[0][1], 77);
            assert!(!types(&frames[1].1).contains(&5), "a delta frame");
            let mut decoder = Decoder::new(Codec::H264).unwrap();
            let mut shown = 0;
            for (_, data) in &frames {
                if let Some(picture) = decoder.decode(data).unwrap() {
                    assert_eq!((picture.width, picture.height), size);
                    shown += 1;
                }
            }
            assert!(
                shown >= 3,
                "{shown} pictures from {engine} at {effort:?} effort"
            );
        }
        engine
    }

    #[test]
    fn encodes_main_profile_h264_that_decodes() {
        for effort in [Effort::Light, Effort::High] {
            assert_eq!(check_h264(effort, (64, 36), false), "x264");
        }
    }

    /// H265 on the GPU, where it has an encoder for it: parameter sets and
    /// an IDR first, a delta, then a forced IDR. Yapfast can't decode H265,
    /// so this reads the units' types.
    #[test]
    fn encodes_h265_on_the_gpu() {
        if Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for H265 here");
            return;
        }
        assert!(Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, engine) = encode_four(Codec::H265, Effort::Light, (320, 180), true);
        eprintln!("encoded with {engine}");
        let types = |data| -> Vec<u8> {
            yapfast_discord::rtp::nal_units(data)
                .iter()
                .map(|unit| (unit[0] >> 1) & 0x3f)
                .collect()
        };
        let idr = |types: &[u8]| types.iter().any(|kind| matches!(kind, 19 | 20));
        let first = types(&frames[0].1);
        assert!(
            first.starts_with(&[32, 33, 34]) && idr(&first),
            "VPS, SPS, PPS, IDR: {first:?}"
        );
        assert!(!idr(&types(&frames[1].1)), "a delta frame");
        assert!(idr(&types(&frames[2].1)), "the forced keyframe");
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// AV1 on the GPU, where it has an encoder for it: a sequence header
    /// before the first frame and the forced keyframe, not the delta, and
    /// OBUs that packetize whole.
    #[test]
    fn encodes_av1_on_the_gpu() {
        if Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for AV1 here");
            return;
        }
        assert!(Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, engine) = encode_four(Codec::Av1, Effort::Light, (320, 180), true);
        eprintln!("encoded with {engine}");
        let sequence = |data| {
            yapfast_discord::rtp::obu_elements(data)
                .expect("OBUs within the frame")
                .first()
                .is_some_and(|element| (element[0] >> 3) & 0x0f == 1)
        };
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
        assert!(sequence(&frames[0].1) && sequence(&frames[2].1));
        assert!(!sequence(&frames[1].1), "a delta frame");
    }

    /// The GPU's encoder where this machine has one, x264 where it hasn't:
    /// the same stream either way.
    #[test]
    fn encodes_h264_on_the_gpu_or_falls_back() {
        let engine = check_h264(Effort::Light, (320, 180), true);
        eprintln!("encoded with {engine}");
    }
}
