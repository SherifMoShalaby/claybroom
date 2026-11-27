//! Photo analyzer: blurry, badly lit and poor-contrast photos, and groups of look-alike shots.
//! Thresholds are Claybroom's own: see MECHANICS.md.

use crate::clean::{say, set_frac, Progress};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// Variance of the Laplacian below this (on a 256 px copy) = blurry.
pub const BLUR_LIMIT: f32 = 60.0;
/// Average brightness (0–255) below / above these = too dark / too bright.
pub const DARK_LIMIT: f32 = 35.0;
pub const BRIGHT_LIMIT: f32 = 225.0;
/// Brightness spread (standard deviation) below this = poor contrast.
pub const CONTRAST_LIMIT: f32 = 18.0;
/// Two photos look alike when their 64-bit difference hashes differ in at most this many bits,
/// they have the same shape, and they were taken within SIMILAR_SECS of each other.
pub const SIMILAR_BITS: u32 = 5;
pub const SIMILAR_SECS: u64 = 600;
pub const SIMILAR_ASPECT: f32 = 0.03;
/// Skip tiny images (icons, thumbnails) and stop after this many photos.
pub const MIN_BYTES: u64 = 50_000;
pub const MAX_PHOTOS: usize = 3000;

pub struct Thumb {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct Photo {
    pub path: PathBuf,
    pub size: u64,
    pub sharpness: f32,
    /// Why it's a bad photo ("Blurry", "Too dark"...), empty for a good one.
    pub reason: &'static str,
    pub selected: bool,
    pub thumb: Arc<Thumb>,
    /// In the Photos app library: its id there ("<uuid>/L0/001"). Such photos are never deleted
    /// as files; they're put in an album in Photos for the person to delete there.
    pub library_id: Option<String>,
    hash: u64,
    aspect: f32,
    taken: u64,
}

#[derive(Default, Clone)]
pub struct PhotoResults {
    pub similar: Vec<Vec<Photo>>,
    pub bad: Vec<Photo>,
}

impl PhotoResults {
    pub fn all(&self) -> impl Iterator<Item = &Photo> {
        self.similar.iter().flatten().chain(self.bad.iter())
    }
    pub fn all_mut(&mut self) -> impl Iterator<Item = &mut Photo> {
        self.similar.iter_mut().flatten().chain(self.bad.iter_mut())
    }
}

pub fn folders() -> Vec<PathBuf> {
    [dirs::picture_dir(), dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()].into_iter().flatten().collect()
}

/// The folders to analyse, with the Photos library's originals when the setting asks (macOS).
pub fn roots(s: &crate::clean::Settings) -> Vec<PathBuf> {
    let mut r = folders();
    if s.photos_library {
        r.extend(library_originals());
    }
    r
}

/// The Photos app library's originals folder (read only: never changed directly).
pub fn library_originals() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    Some(dirs::picture_dir()?.join("Photos Library.photoslibrary/originals")).filter(|p| p.is_dir())
}

/// A file in a Photos library's originals is the media item "<file name without extension>/L0/001".
pub fn library_id(path: &Path) -> Option<String> {
    let mut parts = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
    parts.find(|c| c.ends_with(".photoslibrary"))?;
    if parts.next()? != "originals" {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-').then(|| format!("{stem}/L0/001"))
}

/// JPEG and PNG everywhere; HEIC too on macOS, which converts it with its own `sips` tool.
pub const PHOTO_TYPES: [&str; 3] = ["jpg", "jpeg", "png"];
pub const HEIC_TYPES: [&str; 2] = ["heic", "heif"];

fn is_photo(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| PHOTO_TYPES.contains(&e.as_str()) || heic_readable() && HEIC_TYPES.contains(&e.as_str()))
}

/// HEIC conversions that worked and failed so far: a computer without any converter stops
/// listing HEIC photos after three failures in a row.
static HEIC_OK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static HEIC_FAIL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn heic_readable() -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    cfg!(target_os = "macos") || HEIC_OK.load(Relaxed) > 0 || HEIC_FAIL.load(Relaxed) < 3
}

/// Run a converter quietly (no console window on Windows); true when it succeeded.
fn run(cmd: &mut std::process::Command) -> bool {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000);
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

/// ffmpeg (7 and later read HEIC), scaled to at most 512 px wide.
fn ffmpeg_to_jpeg(src: &Path, dst: &Path) -> bool {
    run(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(src).args(["-vf", "scale='min(512,iw)':-2", "-frames:v", "1"]).arg(dst))
}

/// A HEIC photo as a small JPEG, with the first converter this computer has: macOS's sips;
/// elsewhere ImageMagick, libheif's heif-convert, ffmpeg, or (Windows) its own HEIF decoder from
/// the HEIF Image Extensions.
fn heic_to_jpeg(src: &Path, dst: &Path) -> bool {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    if run(Command::new("sips").args(["-Z", "512", "-s", "format", "jpeg"]).arg(src).arg("--out").arg(dst)) {
        return true;
    }
    if run(Command::new("magick").arg(src).args(["-resize", "512x512>"]).arg(dst)) {
        return true;
    }
    if run(Command::new("heif-convert").args(["-q", "85"]).arg(src).arg(dst)) {
        return true;
    }
    if ffmpeg_to_jpeg(src, dst) {
        return true;
    }
    #[cfg(windows)]
    {
        let q = |p: &Path| p.display().to_string().replace('\'', "''");
        let ps = format!(
            "Add-Type -AssemblyName PresentationCore; \
             $d=[System.Windows.Media.Imaging.BitmapDecoder]::Create([Uri]::new('{}'),[System.Windows.Media.Imaging.BitmapCreateOptions]::None,[System.Windows.Media.Imaging.BitmapCacheOption]::OnLoad); \
             $f=$d.Frames[0]; $k=[Math]::Min(1.0, 512.0/[Math]::Max($f.PixelWidth,$f.PixelHeight)); \
             $t=[System.Windows.Media.Imaging.TransformedBitmap]::new($f,[System.Windows.Media.ScaleTransform]::new($k,$k)); \
             $e=[System.Windows.Media.Imaging.JpegBitmapEncoder]::new(); $e.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($t)); \
             $s=[IO.File]::Create('{}'); $e.Save($s); $s.Close()",
            q(src),
            q(dst)
        );
        if run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps])) {
            return true;
        }
    }
    false
}

fn is_heic(p: &Path) -> bool {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).is_some_and(|e| e == "heic" || e == "heif")
}

/// Open a photo: directly, or for HEIC through a small JPEG copy made by a converter.
fn open_photo(path: &Path) -> Option<image::DynamicImage> {
    if !is_heic(path) {
        return image::open(path).ok();
    }
    use std::sync::atomic::Ordering::Relaxed;
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tmp = std::env::temp_dir().join(format!("claybroom-heic-{}-{}.jpg", std::process::id(), N.fetch_add(1, Relaxed)));
    let img = if heic_to_jpeg(path, &tmp) { image::open(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    if img.is_some() { HEIC_OK.fetch_add(1, Relaxed) } else { HEIC_FAIL.fetch_add(1, Relaxed) };
    img
}

/// The quality measures of one image, read from a 256 px grey copy.
pub struct Measures {
    pub brightness: f32,
    pub contrast: f32,
    pub sharpness: f32,
    pub hash: u64,
}

pub fn measure(img: &image::DynamicImage) -> Measures {
    let gray = img.thumbnail(256, 256).to_luma8();
    let (w, h) = gray.dimensions();
    let px: Vec<f32> = gray.pixels().map(|p| p.0[0] as f32).collect();
    let n = px.len().max(1) as f32;
    let brightness = px.iter().sum::<f32>() / n;
    let contrast = (px.iter().map(|v| (v - brightness).powi(2)).sum::<f32>() / n).sqrt();
    // Variance of the Laplacian: sharp edges give big values, blur gives small ones.
    let mut lap = Vec::with_capacity(px.len());
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            lap.push(4.0 * px[i] - px[i - 1] - px[i + 1] - px[i - w as usize] - px[i + w as usize]);
        }
    }
    let m = lap.len().max(1) as f32;
    let mean = lap.iter().sum::<f32>() / m;
    let sharpness = lap.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / m;
    // Difference hash: is each pixel brighter than its right neighbour, on a 9×8 copy.
    let small = image::imageops::resize(&gray, 9, 8, image::imageops::FilterType::Triangle);
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash = hash << 1 | (small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0]) as u64;
        }
    }
    Measures { brightness, contrast, sharpness, hash }
}

pub fn reason(m: &Measures) -> &'static str {
    match () {
        _ if m.brightness < DARK_LIMIT => "Too dark",
        _ if m.brightness > BRIGHT_LIMIT => "Too bright",
        _ if m.sharpness < BLUR_LIMIT => "Blurry",
        _ if m.contrast < CONTRAST_LIMIT => "Poor contrast",
        _ => "",
    }
}

fn analyse(path: &Path, size: u64) -> Option<Photo> {
    let img = open_photo(path)?;
    let aspect = img.width() as f32 / img.height().max(1) as f32;
    let taken = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let m = measure(&img);
    let t = img.thumbnail(140, 140).to_rgba8();
    let thumb = Arc::new(Thumb { w: t.width() as usize, h: t.height() as usize, rgba: t.into_raw() });
    Some(Photo { library_id: library_id(path), path: path.to_path_buf(), size, sharpness: m.sharpness, reason: reason(&m), selected: false, thumb, hash: m.hash, aspect, taken })
}

/// Group look-alike photos and list the bad ones that aren't in a group. A photo joins a group
/// only if it matches the group's first photo (no chains of "each looks like the next"), so a
/// group is one moment: a burst or a few shots of the same thing. In each group every photo but
/// the sharpest is ticked; bad photos start unticked.
pub fn sort_out(mut photos: Vec<Photo>) -> PhotoResults {
    photos.sort_by_key(|p| p.taken);
    let mut groups: Vec<Vec<Photo>> = vec![];
    for ph in photos {
        // Groups are in time order, so only the last few can still be within SIMILAR_SECS.
        let hit = groups.iter_mut().rev().take_while(|g| ph.taken.saturating_sub(g[0].taken) <= SIMILAR_SECS).find(|g| {
            let seed = &g[0];
            (seed.hash ^ ph.hash).count_ones() <= SIMILAR_BITS && (seed.aspect - ph.aspect).abs() <= SIMILAR_ASPECT
        });
        match hit {
            Some(g) => g.push(ph),
            None => groups.push(vec![ph]),
        }
    }
    let mut out = PhotoResults::default();
    for mut g in groups {
        if g.len() > 1 {
            g.sort_by(|a, b| b.sharpness.total_cmp(&a.sharpness));
            g.iter_mut().skip(1).for_each(|p| p.selected = true);
            out.similar.push(g);
        } else if !g[0].reason.is_empty() {
            out.bad.extend(g);
        }
    }
    out.similar.sort_by(|a, b| b.iter().map(|p| p.size).sum::<u64>().cmp(&a.iter().map(|p| p.size).sum::<u64>()));
    out.bad.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

pub fn scan(roots: &[PathBuf], p: &Progress) -> PhotoResults {
    say(p, "Looking for photos…");
    let mut files = vec![];
    for r in roots {
        let mut it = WalkDir::new(r).into_iter();
        while let Some(Ok(e)) = it.next() {
            let name = e.file_name().to_string_lossy();
            // Hidden folders, app bundles and photo libraries are not ours to touch.
            if e.depth() > 0 && (name.starts_with('.') || name.ends_with(".photoslibrary") || name.ends_with(".app")) {
                if e.file_type().is_dir() {
                    it.skip_current_dir();
                }
                continue;
            }
            if e.file_type().is_file() && is_photo(e.path()) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                if size >= MIN_BYTES {
                    files.push((e.path().to_path_buf(), size));
                }
            }
        }
        if files.len() >= MAX_PHOTOS {
            break;
        }
    }
    files.truncate(MAX_PHOTOS);
    let total = files.len().max(1);
    let done = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(threads).max(1);
    let photos: Vec<Photo> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                let done = &done;
                s.spawn(move || {
                    part.iter()
                        .filter_map(|(path, size)| {
                            let k = done.fetch_add(1, Ordering::Relaxed) + 1;
                            set_frac(p, 0.95 * k as f32 / total as f32);
                            say(p, format!("Analyzing {}", path.display()));
                            analyse(path, *size)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    say(p, "Comparing photos…");
    let out = sort_out(photos);
    set_frac(p, 1.0);
    say(p, "");
    out
}

/// What the Statistics page counts: (bytes freed, photos removed).
pub fn clean(res: &PhotoResults, to_trash: bool, p: &Progress) -> crate::clean::CleanReport {
    let sel: Vec<&Photo> = res.all().filter(|x| x.selected).collect();
    let mut r = crate::clean::CleanReport::default();
    // Photos-library photos go to an album in Photos instead (deleting library files breaks it).
    let ids: Vec<String> = sel.iter().filter_map(|x| x.library_id.clone()).collect();
    if !ids.is_empty() {
        say(p, "Putting the library photos in an album in Photos…");
        match crate::shell::photos_review_album(&ids) {
            Ok(n) => r.notes.push(format!(
                "{n} photo(s) from your Photos library are in the album \"{}\" in Photos: open it, select all and delete them there (Photos keeps them in Recently Deleted for 30 days).",
                crate::shell::REVIEW_ALBUM
            )),
            Err(e) => r.notes.push(format!("The Photos library photos were left alone: {e}.")),
        }
    }
    let sel: Vec<&Photo> = sel.into_iter().filter(|x| x.library_id.is_none()).collect();
    for (i, ph) in sel.iter().enumerate() {
        set_frac(p, (i + 1) as f32 / sel.len().max(1) as f32);
        say(p, format!("Removing {}", ph.path.display()));
        let ok = if to_trash { trash::delete(&ph.path).is_ok() } else { std::fs::remove_file(&ph.path).is_ok() };
        if ok {
            r.freed += ph.size;
            r.removed += 1;
        } else {
            r.failed += 1;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GrayImage, Luma};

    fn checker(dark: u8, light: u8) -> DynamicImage {
        DynamicImage::ImageLuma8(GrayImage::from_fn(256, 256, |x, y| Luma([if (x / 8 + y / 8) % 2 == 0 { dark } else { light }])))
    }

    #[test]
    fn photo_rules() {
        // A sharp, well-lit, contrasty picture is fine.
        assert_eq!(reason(&measure(&checker(30, 220))), "");
        // A flat grey picture has no edges and no contrast: blurry.
        assert_eq!(reason(&measure(&checker(128, 128))), "Blurry");
        assert_eq!(reason(&measure(&checker(5, 20))), "Too dark");
        assert_eq!(reason(&measure(&checker(235, 250))), "Too bright");
        // The same picture slightly brighter looks alike; a shifted pattern doesn't.
        let a = measure(&checker(30, 220)).hash;
        let b = measure(&checker(40, 230)).hash;
        assert!((a ^ b).count_ones() <= SIMILAR_BITS);
    }

    fn photo(hash: u64, taken: u64, sharpness: f32) -> Photo {
        let thumb = Arc::new(Thumb { w: 1, h: 1, rgba: vec![0; 4] });
        Photo { library_id: None, path: PathBuf::from(format!("{hash}-{taken}")), size: 1, sharpness, reason: "", selected: false, thumb, hash, aspect: 1.5, taken }
    }

    #[test]
    fn similar_groups_are_one_moment_without_chains() {
        let a = 0u64;
        let b = 0b1111; // 4 bits from a: alike
        let c = 0b1111_1111; // 4 bits from b but 8 from a: not alike to the group's first photo
        let r = sort_out(vec![photo(a, 1000, 10.0), photo(b, 1010, 50.0), photo(c, 1020, 10.0), photo(a, 1000 + 3 * 86_400, 10.0)]);
        assert_eq!(r.similar.len(), 1, "c must not chain on, and the photo days later is another moment");
        let g = &r.similar[0];
        assert_eq!(g.len(), 2);
        assert!(!g[0].selected && g[0].sharpness == 50.0, "the sharpest stays unticked");
        assert!(g[1].selected);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod heic {
    /// A HEIC photo is read through `sips`: make one from a test picture, then analyse it.
    #[test]
    fn heic_photos_are_read_on_macos() {
        let dir = std::env::temp_dir().join(format!("claybroom-heic-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 90]))).save(&png).unwrap();
        let heic = dir.join("a.heic");
        let made = std::process::Command::new("sips").args(["-s", "format", "heic"]).arg(&png).arg("--out").arg(&heic).output().unwrap();
        assert!(made.status.success() && heic.exists(), "sips couldn't write HEIC");
        let p = super::analyse(&heic, 1).expect("HEIC photo read");
        assert!((p.aspect - 64.0 / 48.0).abs() < 0.05);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod library {
    #[test]
    fn library_originals_have_photos_ids() {
        let p = std::path::Path::new("/Users/a/Pictures/Photos Library.photoslibrary/originals/A/A1B2C3D4-0000-4E5F-8888-123456789ABC.heic");
        assert_eq!(super::library_id(p).as_deref(), Some("A1B2C3D4-0000-4E5F-8888-123456789ABC/L0/001"));
        assert_eq!(super::library_id(std::path::Path::new("/Users/a/Pictures/holiday.jpg")), None);
        assert_eq!(super::library_id(std::path::Path::new("/x/Photos Library.photoslibrary/resources/a.jpg")), None);
    }
}

#[cfg(test)]
mod heic_elsewhere {
    /// The converter Windows and Linux fall back on (ffmpeg) reads HEIC; checked here when ffmpeg
    /// and macOS's sips (to make a test HEIC) are both present, skipped otherwise.
    #[test]
    fn ffmpeg_reads_heic_when_present() {
        let has = |t: &str| std::process::Command::new(t).arg("-version").output().is_ok();
        if !cfg!(target_os = "macos") || !has("ffmpeg") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("claybroom-ffheic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(96, 64, |x, y| image::Rgb([(x * 2) as u8, (y * 3) as u8, 70]))).save(&png).unwrap();
        let heic = dir.join("a.heic");
        assert!(std::process::Command::new("sips").args(["-s", "format", "heic"]).arg(&png).arg("--out").arg(&heic).output().unwrap().status.success());
        let out = dir.join("a.jpg");
        assert!(super::ffmpeg_to_jpeg(&heic, &out));
        let img = image::open(&out).unwrap();
        assert_eq!((img.width(), img.height()), (96, 64));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
