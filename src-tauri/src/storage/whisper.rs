//! Voice-message transcription — desktop side.
//!
//! Strategy: we don't statically link whisper.cpp (its Rust binding wants
//! libclang at build time, which is a heavy Windows toolchain ask). Instead
//! we ship the user's own `whisper-cli.exe` (from the whisper.cpp releases)
//! into an app-data folder, decode compressed audio to WAV via symphonia,
//! then invoke the CLI as a subprocess and read its `.txt` output.
//!
//! This module owns:
//! - path resolution for the CLI + ggml model,
//! - decoding an arbitrary audio blob to a 16-kHz mono WAV file whisper can
//!   read directly (symphonia handles Android's AAC-in-MP4 blobs; the
//!   desktop's own webm/opus blobs will fall through until we land the
//!   opus→AAC transcode TODO),
//! - a streaming HTTP download of the ggml model,
//! - the actual transcription entry point.
//!
//! The command layer in `commands.rs` wraps these primitives; nothing here
//! knows about tauri, keystores, or event authoring.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Pinned to whisper.cpp's canonical multilingual tiny model. HTTPS-only,
/// served by HuggingFace under ggerganov's own repo.
pub const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin";

/// Approximate on-disk size the UI announces before download. Not verified.
pub const MODEL_APPROX_SIZE_BYTES: u64 = 77_691_713; // ~74 MB (multilingual tiny)

/// Every path whisper cares about, rooted at `<app-data>/whisper/`.
pub struct WhisperPaths {
    pub dir: PathBuf,
    pub cli: PathBuf,
    pub model: PathBuf,
}

impl WhisperPaths {
    pub fn from_data_root(root: &Path) -> Self {
        let dir = root.join("whisper");
        let cli = dir.join(if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" });
        let model = dir.join("ggml-tiny.bin");
        Self { dir, cli, model }
    }

    pub fn cli_present(&self) -> bool {
        self.cli.is_file()
    }
    pub fn model_present(&self) -> bool {
        self.model.is_file()
    }
    pub fn model_size(&self) -> Option<u64> {
        fs::metadata(&self.model).ok().map(|m| m.len())
    }
    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.dir).context("create whisper data dir")?;
        Ok(())
    }

    /// Locate `ffmpeg(.exe)` — used as a fallback decoder for formats symphonia
    /// can't handle (notably webm/opus, which is what the desktop's own
    /// MediaRecorder emits). Order of preference: bundled next to whisper-cli,
    /// then on PATH.
    pub fn locate_ffmpeg(&self) -> Option<PathBuf> {
        let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
        let local = self.dir.join(name);
        if local.is_file() {
            return Some(local);
        }
        // Probe PATH by spawning `ffmpeg -version` — cheapest reliable check
        // across platforms. If it succeeds, `Command::new("ffmpeg")` will
        // resolve the same binary at run time.
        let ok = Command::new("ffmpeg")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            Some(PathBuf::from("ffmpeg"))
        } else {
            None
        }
    }
}

/// Public UI-facing status of the local install. Field names match the TS
/// `WhisperStatus` type in `src/lib/ipc.ts` — camelCase over the tauri bridge.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperStatus {
    pub cli_present: bool,
    pub model_present: bool,
    pub model_size_bytes: Option<u64>,
    pub dir: String,
    /// `true` when ffmpeg is reachable (bundled in the whisper dir OR on PATH).
    /// Required for decoding webm/opus voice — the desktop's own MediaRecorder
    /// output. Phone-recorded voice (real AAC/MP4) transcribes without it.
    pub ffmpeg_present: bool,
}

impl WhisperStatus {
    pub fn probe(paths: &WhisperPaths) -> Self {
        Self {
            cli_present: paths.cli_present(),
            model_present: paths.model_present(),
            model_size_bytes: paths.model_size(),
            dir: paths.dir.display().to_string(),
            ffmpeg_present: paths.locate_ffmpeg().is_some(),
        }
    }
}

/// Blocking download of the ggml model to a temp file, then rename into place
/// on success. `progress` is invoked with `(bytes_read, total_or_zero)` at
/// most every ~64 KiB to feed a progress bar; the closure is `Send + Sync`
/// so callers can push into a tokio mpsc.
pub async fn download_model<F>(paths: &WhisperPaths, mut progress: F) -> Result<()>
where
    F: FnMut(u64, u64) + Send + 'static,
{
    paths.ensure_dir()?;
    let tmp = paths.dir.join("ggml-tiny.bin.download");
    // Clean any prior partial.
    let _ = fs::remove_file(&tmp);

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .get(MODEL_URL)
        .send()
        .await
        .context("start model download")?;
    if !resp.status().is_success() {
        bail!("model download HTTP {}", resp.status());
    }
    let total = resp.content_length().unwrap_or(0);

    // Stream to disk.
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .context("open model tmp file")?;
    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;
    let mut last_report: u64 = 0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("read chunk")?;
        file.write_all(&bytes).await.context("write chunk")?;
        written += bytes.len() as u64;
        if written - last_report >= 64 * 1024 || written == total {
            progress(written, total);
            last_report = written;
        }
    }
    file.flush().await?;
    drop(file);

    // Sanity: a truncated download will produce a file smaller than a whisper
    // model header (~million bytes). Fail loud instead of leaving something
    // corrupt on disk.
    let size = fs::metadata(&tmp)?.len();
    if size < 1_000_000 {
        let _ = fs::remove_file(&tmp);
        bail!("model download suspiciously small ({} bytes) — retry", size);
    }
    fs::rename(&tmp, &paths.model).context("finalize model file")?;
    Ok(())
}

/// Decode an arbitrary audio blob to a 16-kHz mono PCM-16 WAV file at `dest`.
/// Whisper's CLI reads WAV directly so this is all we need before invoking it.
///
/// Uses pure-Rust symphonia when it can (Android's AAC/MP4 blobs), falls back
/// to `ffmpeg` for formats symphonia can't decode (webm/opus from the
/// desktop's own MediaRecorder). `ffmpeg_hint` is only consulted on fallback;
/// pass `None` to skip the fallback and fail loud instead.
pub fn decode_to_wav_16k(
    input_bytes: Vec<u8>,
    dest: &Path,
    ffmpeg_hint: Option<&Path>,
) -> Result<()> {
    // Try symphonia first — fast, in-process, no external deps.
    match decode_to_pcm_16k_mono(input_bytes.clone()) {
        Ok(samples) => {
            write_wav_pcm16_mono_16k(&samples, dest)?;
            return Ok(());
        }
        Err(sym_err) => {
            // Fall through to ffmpeg. Preserve the original error to include in
            // the eventual bail!() if ffmpeg is also unavailable.
            if let Some(ff) = ffmpeg_hint {
                decode_via_ffmpeg(ff, &input_bytes, dest).with_context(|| {
                    format!("symphonia failed ({sym_err}); ffmpeg fallback also failed")
                })?;
                return Ok(());
            }
            bail!(
                "{sym_err} — and ffmpeg was not found. Install ffmpeg (put ffmpeg.exe next to whisper-cli.exe, or on PATH) so the desktop can decode webm/opus voice messages"
            );
        }
    }
}

/// Fallback path: pipe the input bytes into `ffmpeg` on stdin, ask it to
/// produce a 16 kHz mono PCM WAV, and drop the result at `dest`. Handles any
/// container/codec ffmpeg knows about — critically opus-in-webm from the
/// desktop's own `MediaRecorder`.
fn decode_via_ffmpeg(ff: &Path, input_bytes: &[u8], dest: &Path) -> Result<()> {
    use std::process::Stdio;

    // Write input to a temp file rather than piping — some ffmpeg builds
    // struggle with unseekable stdin on webm, and this keeps the invocation
    // trivially reproducible from the command line for debugging.
    let tmp_in = dest.with_extension("in.tmp");
    std::fs::write(&tmp_in, input_bytes).context("write ffmpeg input")?;

    let mut cmd = Command::new(ff);
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel").arg("error")
        .arg("-i").arg(&tmp_in)
        .arg("-ar").arg("16000")
        .arg("-ac").arg("1")
        .arg("-c:a").arg("pcm_s16le")
        .arg(dest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = cmd.output().context("spawn ffmpeg")?;
    let _ = std::fs::remove_file(&tmp_in);
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg exit {}: {}", output.status, err.trim());
    }
    Ok(())
}

fn decode_to_pcm_16k_mono(input_bytes: Vec<u8>) -> Result<Vec<f32>> {
    let cursor = std::io::Cursor::new(input_bytes);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!("symphonia probe failed: {e} (unsupported container? webm/opus is not yet supported)"))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no decodable audio track"))?;
    let track_id = track.id;
    let src_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("audio track has no sample rate"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("no decoder: {e}"))?;

    let mut mono_f32: Vec<f32> = Vec::with_capacity(src_rate as usize * 4);

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(anyhow!("packet read: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // skip a bad packet
            Err(e) => return Err(anyhow!("decode: {e}")),
        };
        push_mono_f32(&decoded, &mut mono_f32);
    }

    if mono_f32.is_empty() {
        bail!("decoded audio is empty");
    }

    // Resample to 16 kHz with a naive linear interp — good enough for speech.
    // Whisper is robust to low-quality resampling; we don't need a poly-phase.
    let out = resample_linear(&mono_f32, src_rate, 16_000);
    Ok(out)
}

fn push_mono_f32(buf: &AudioBufferRef<'_>, out: &mut Vec<f32>) {
    use symphonia::core::audio::AudioBufferRef::*;
    macro_rules! mix {
        ($src:expr, $to_f32:expr) => {{
            let frames = $src.frames();
            let ch = $src.spec().channels.count();
            for i in 0..frames {
                let mut sum = 0.0f32;
                for c in 0..ch {
                    let s = $src.chan(c)[i];
                    sum += $to_f32(s);
                }
                out.push(sum / ch as f32);
            }
        }};
    }
    match buf {
        F32(b) => mix!(b, |s: f32| s),
        F64(b) => mix!(b, |s: f64| s as f32),
        S8(b) => mix!(b, |s: i8| s as f32 / i8::MAX as f32),
        S16(b) => mix!(b, |s: i16| s as f32 / i16::MAX as f32),
        S24(b) => mix!(b, |s: symphonia::core::sample::i24| s.inner() as f32 / (1 << 23) as f32),
        S32(b) => mix!(b, |s: i32| s as f32 / i32::MAX as f32),
        U8(b) => mix!(b, |s: u8| (s as f32 - 128.0) / 128.0),
        U16(b) => mix!(b, |s: u16| (s as f32 - 32768.0) / 32768.0),
        U24(b) => mix!(b, |s: symphonia::core::sample::u24| (s.inner() as f32 - (1 << 23) as f32) / (1 << 23) as f32),
        U32(b) => mix!(b, |s: u32| (s as f32 - u32::MAX as f32 / 2.0) / (u32::MAX as f32 / 2.0)),
    }
}

fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate {
        return input.to_vec();
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

fn write_wav_pcm16_mono_16k(samples: &[f32], dest: &Path) -> Result<()> {
    // Canonical PCM WAV: 44-byte header, then interleaved little-endian s16.
    let mut file = BufWriter::new(File::create(dest).context("create wav")?);
    let n_samples = samples.len() as u32;
    let byte_rate: u32 = 16_000 * 2; // sample_rate * bytes_per_sample * channels
    let data_bytes = n_samples * 2;
    let riff_size = 36 + data_bytes;

    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&1u16.to_le_bytes())?; // mono
    file.write_all(&16_000u32.to_le_bytes())?; // sample rate
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?; // block align
    file.write_all(&16u16.to_le_bytes())?; // bits per sample

    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;

    for &s in samples {
        let clamped = s.max(-1.0).min(1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        file.write_all(&v.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

/// Run `whisper-cli` on a WAV file already sitting on disk. Returns the
/// transcript text (trimmed). Blocking; call from `spawn_blocking`.
pub fn run_whisper_cli(paths: &WhisperPaths, wav: &Path) -> Result<String> {
    if !paths.cli_present() {
        bail!(
            "whisper-cli not installed. Put whisper-cli(.exe) in {}",
            paths.dir.display()
        );
    }
    if !paths.model_present() {
        bail!("ggml-tiny.bin model not downloaded — enable in Settings first");
    }
    let stem = wav.with_extension("");
    let out_txt = wav.with_extension("txt");
    // Clean any old output so we don't accidentally read a stale run.
    let _ = fs::remove_file(&out_txt);

    // Preserve the exact argv for the error message — invaluable when whisper
    // exits non-zero with empty stderr (some builds write to stdout only, or
    // die on a DLL-load error before any I/O). Set CWD to the whisper dir so
    // relative model/DLL lookups behave the same as running it by hand.
    let args: Vec<std::ffi::OsString> = vec![
        "-m".into(),
        paths.model.clone().into_os_string(),
        "-f".into(),
        wav.to_path_buf().into_os_string(),
        "-otxt".into(),
        "-of".into(),
        stem.clone().into_os_string(),
        "-nt".into(),
    ];

    let output = Command::new(&paths.cli)
        .args(&args)
        .current_dir(&paths.dir)
        .output()
        .context("spawn whisper-cli")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let raw_code = output.status.code();
        // Windows loader failures — same numeric exit codes have unrelated
        // meanings on macOS/Linux, so gate this decoding on target_os.
        #[cfg(windows)]
        if let Some(c) = raw_code {
            match c as u32 {
                // 0xC0000135 == STATUS_DLL_NOT_FOUND — sibling DLLs missing.
                0xC0000135 => bail!(
                    "whisper-cli couldn't start — it's missing its sibling DLLs (STATUS_DLL_NOT_FOUND). Extract every file from the whisper.cpp release zip into {} (whisper-cli.exe, whisper.dll, ggml.dll, ggml-base.dll, ggml-cpu.dll, and any others). You likely copied just the exe.",
                    paths.dir.display()
                ),
                // 0xC000007B == STATUS_INVALID_IMAGE_FORMAT — arch mismatch.
                0xC000007B => bail!(
                    "whisper-cli couldn't start — invalid image format (STATUS_INVALID_IMAGE_FORMAT). Usually means a corrupted download, or you mixed 32-bit and 64-bit files. Re-download the whisper.cpp release zip and re-extract."
                ),
                _ => {}
            }
        }
        // POSIX shell convention: 127 = "command not found" (dynamic linker
        // couldn't resolve a shared library, typically). Give a hint that
        // matches macOS/Linux terminology.
        #[cfg(not(windows))]
        if raw_code == Some(127) {
            bail!(
                "whisper-cli couldn't start (exit 127 — 'command not found' / dynamic-link failure). Make sure whisper-cli plus its sibling shared libraries (*.dylib on macOS, *.so on Linux) are all extracted into {}. On Linux you may also need to `chmod +x whisper-cli`.",
                paths.dir.display()
            );
        }
        let code = raw_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signalled".to_string());
        let argv: Vec<String> = args.iter().map(|s| s.to_string_lossy().into()).collect();
        bail!(
            "whisper-cli exited {code}. cmd: {} {}. stderr: {}. stdout: {}",
            paths.cli.display(),
            argv.join(" "),
            if stderr.trim().is_empty() { "(empty)" } else { stderr.trim() },
            if stdout.trim().is_empty() { "(empty)" } else { stdout.trim() },
        );
    }

    // Some whisper-cli builds also emit the transcript on stdout instead of
    // (or in addition to) writing the -otxt file. Prefer the file when it
    // exists — it's what we asked for and it's already `-nt` cleaned — but
    // fall back to stdout so a build that doesn't honour `-of` still works.
    if out_txt.is_file() {
        let mut text = String::new();
        File::open(&out_txt)
            .context("read whisper txt output")?
            .read_to_string(&mut text)?;
        return Ok(text.trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        bail!(
            "whisper-cli produced no output file ({}) and empty stdout",
            out_txt.display()
        );
    }
    Ok(stdout)
}

/// End-to-end: raw audio bytes → transcript. Writes temp files under a
/// scratch dir it cleans up on both success and failure paths.
pub fn transcribe_bytes(paths: &WhisperPaths, audio_bytes: Vec<u8>) -> Result<String> {
    paths.ensure_dir()?;
    let scratch = paths.dir.join("scratch");
    fs::create_dir_all(&scratch)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let wav = scratch.join(format!("in_{stamp}.wav"));
    let txt = wav.with_extension("txt");

    let ffmpeg = paths.locate_ffmpeg();
    let result = (|| -> Result<String> {
        decode_to_wav_16k(audio_bytes, &wav, ffmpeg.as_deref())?;
        run_whisper_cli(paths, &wav)
    })();

    let _ = fs::remove_file(&wav);
    let _ = fs::remove_file(&txt);

    result
}
