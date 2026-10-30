//! Full photo previews and durable, ID-only upscale provenance.
use crate::{
    models::{MediaItem, Source},
    providers, storage,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImage {
    pub url: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPreview {
    #[serde(flatten)]
    pub image: PreviewImage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<PreviewImage>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub comparison_unavailable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Original {
    source_id: String,
    media_id: String,
}

type Associations = BTreeMap<String, Original>;

pub(crate) struct Context<'a> {
    pub root: &'a Path,
    pub client: &'a reqwest::blocking::Client,
    pub sources: &'a [Source],
    pub known: &'a [MediaItem],
    pub network_allowed: bool,
}
pub(crate) struct PreparedImage {
    pub media: MediaItem,
    pub source: Source,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    extension: &'static str,
}
pub(crate) struct PreparedPreview {
    pub image: PreparedImage,
    pub before: Option<PreparedImage>,
    pub comparison_unavailable: bool,
}
impl PreparedPreview {
    pub(crate) fn images(&self) -> impl Iterator<Item = &PreparedImage> {
        std::iter::once(&self.image).chain(self.before.iter())
    }
    pub(crate) fn publish(&self, cache: &Path) -> Result<PhotoPreview, String> {
        let publish = |image: &PreparedImage| {
            let path = cache.join(format!("{}-photo.{}", image.media.id, image.extension));
            storage::atomic_write(&path, &image.bytes)?;
            Ok::<_, String>(PreviewImage {
                url: path.to_string_lossy().into_owned(),
                width: image.width,
                height: image.height,
            })
        };
        Ok(PhotoPreview {
            image: publish(&self.image)?,
            before: self.before.as_ref().map(publish).transpose()?,
            comparison_unavailable: self.comparison_unavailable,
        })
    }
}

const MAX_ASSOCIATIONS: u64 = 4 * 1024 * 1024;
const MAX_SAVED_BYTES: u64 = 64 * 1024 * 1024;
fn valid_media_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn valid_source_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "Image is no longer accessible.".to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("Photo exceeds its byte limit.".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Photo exceeds its byte limit.".into());
    }
    Ok(bytes)
}
fn read_associations(collection: &Path) -> Result<Associations, String> {
    let path = collection.join("comparisons.json");
    if !path.exists() {
        return Ok(Associations::new());
    }
    serde_json::from_slice(&read_bounded(&path, MAX_ASSOCIATIONS)?)
        .map_err(|_| "Invalid original image associations.".into())
}
/// Called under the worker's collection publication lock. Never persist a source object or URL.
pub(crate) fn record_original(
    collection: &Path,
    filename: &str,
    media: &MediaItem,
) -> Result<(), String> {
    if Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(filename)
        || !filename.ends_with(".jpg")
        || !valid_media_id(&media.id)
        || !valid_source_id(&media.source_id)
    {
        return Err("Invalid original image association.".into());
    }
    let mut associations = read_associations(collection)?;
    if associations.len() >= 10000 && !associations.contains_key(filename) {
        return Err("Too many original image associations.".into());
    }
    associations.insert(
        filename.into(),
        Original {
            source_id: media.source_id.clone(),
            media_id: media.id.clone(),
        },
    );
    let bytes = serde_json::to_vec(&associations).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_ASSOCIATIONS {
        return Err("Original image associations exceed their size limit.".into());
    }
    storage::atomic_write(&collection.join("comparisons.json"), &bytes)
}
fn resolve(
    context: &Context<'_>,
    id: &str,
    source_id: &str,
) -> Result<(MediaItem, Source), String> {
    if !valid_media_id(id) || !valid_source_id(source_id) {
        return Err("Invalid photo identity.".into());
    }
    let source = context
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or("Photo source is no longer available.")?;
    // Explicit viewing does not alter wallpaper eligibility. Local IDs are re-authorized
    // through a fresh provider listing, including after source disable/reopen.
    if source.kind != "local" && !context.network_allowed {
        return Err("Waiting for Wi-Fi before downloading this photo.".into());
    }
    let known = (source.kind != "local" && source.kind != "photos")
        .then(|| {
            context
                .known
                .iter()
                .find(|media| media.id == id && media.source_id == source_id)
                .cloned()
        })
        .flatten();
    let media = match known {
        Some(media) => media,
        None => providers::list_source(context.client, source)?
            .into_iter()
            .find(|media| media.id == id && media.source_id == source_id)
            .ok_or("Photo is no longer in this source.")?,
    };
    Ok((media, source.clone()))
}
fn collection(
    context: &Context<'_>,
    media: &MediaItem,
    source: &Source,
) -> Result<Option<PathBuf>, String> {
    if source.kind != "local" {
        return Ok(None);
    }
    let path = Path::new(&media.url)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let base = context.root.join("upscaled");
    let Ok(base) = base.canonicalize() else {
        return Ok(None);
    };
    if !path.starts_with(&base) {
        return Ok(None);
    }
    let parent = path.parent().ok_or("Invalid saved photo path.")?;
    let id = parent
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid saved collection.")?;
    let checked = crate::upscale::checked_collection_path(context.root, id)?;
    if checked != parent {
        return Err("Saved photo is outside its collection.".into());
    }
    Ok(Some(checked))
}
fn prepare_image(
    context: &Context<'_>,
    media: MediaItem,
    source: Source,
    saved: bool,
) -> Result<PreparedImage, String> {
    use image::{GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};
    let bytes = if source.kind == "local" {
        read_bounded(
            Path::new(&media.url),
            if saved {
                MAX_SAVED_BYTES
            } else {
                providers::MAX_IMAGE_BYTES
            },
        )?
    } else {
        providers::download(context.client, &media, &source)?
    };
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "Unknown image format.".to_string())?;
    let format = reader.format().ok_or("Unknown image format.")?;
    let dimensions = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    // Bound allocation before decoding, including crafted compressed images.
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 > 16384
        || dimensions.1 > 16384
        || u64::from(dimensions.0) * u64::from(dimensions.1) > 32_000_000
    {
        return Err("Photo exceeds the safe decode dimensions.".into());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    let (width, height) = image.dimensions();
    let (bytes, extension, width, height) = if !saved
        && (width > 3840 || height > 3840 || matches!(format, ImageFormat::Gif | ImageFormat::WebP))
    {
        let image = image.thumbnail(3840, 3840);
        let mut output = Cursor::new(Vec::new());
        let format = if image.color().has_alpha() {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        image
            .write_to(&mut output, format)
            .map_err(|e| e.to_string())?;
        (
            output.into_inner(),
            if format == ImageFormat::Png {
                "png"
            } else {
                "jpg"
            },
            image.width(),
            image.height(),
        )
    } else {
        let extension = match format {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::WebP => "webp",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Gif => "gif",
            _ => return Err("Unsupported photo format.".into()),
        };
        (bytes, extension, width, height)
    };
    Ok(PreparedImage {
        media,
        source,
        bytes,
        width,
        height,
        extension,
    })
}
pub(crate) fn prepare(
    context: &Context<'_>,
    id: &str,
    source_id: &str,
) -> Result<PreparedPreview, String> {
    let (media, source) = resolve(context, id, source_id)?;
    let collection = collection(context, &media, &source)?;
    let original = collection
        .as_ref()
        .and_then(|path| read_associations(path).ok())
        .and_then(|map| {
            Path::new(&media.url)
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| map.get(name).cloned())
        });
    let image = prepare_image(context, media, source, collection.is_some())?;
    // A missing original never hides the durable saved image. Legacy collections
    // have no association; filenames and dimensions are never used to guess one.
    let before = original.and_then(|original| {
        resolve(context, &original.media_id, &original.source_id)
            .and_then(|(media, source)| prepare_image(context, media, source, false))
            .ok()
    });
    let comparison_unavailable = collection.is_some() && before.is_none();
    Ok(PreparedPreview {
        image,
        before,
        comparison_unavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat};
    const JOB: &str = "0123456789abcdef0123456789abcdef";
    struct Fixture {
        temp: tempfile::TempDir,
        sources: Vec<Source>,
        client: reqwest::blocking::Client,
        original: MediaItem,
        result: MediaItem,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let originals = temp.path().join("originals");
            let collection = temp.path().join("upscaled").join(JOB);
            fs::create_dir_all(&originals).unwrap();
            fs::create_dir_all(&collection).unwrap();
            save(&originals.join("original.png"), 960, 540);
            save(&collection.join("001.jpg"), 3840, 2160);
            let sources: Vec<Source> = [("original", originals), ("upscale-saved", collection)].into_iter().map(|(id, location)| serde_json::from_value(serde_json::json!({"id":id,"name":id,"kind":"local","location":location,"enabled":false})).unwrap()).collect();
            let original = providers::list_source(&providers::client().unwrap(), &sources[0])
                .unwrap()
                .remove(0);
            let result = providers::list_source(&providers::client().unwrap(), &sources[1])
                .unwrap()
                .remove(0);
            Self {
                temp,
                sources,
                client: providers::client().unwrap(),
                original,
                result,
            }
        }
        fn context(&self) -> Context<'_> {
            Context {
                root: self.temp.path(),
                client: &self.client,
                sources: &self.sources,
                known: &[],
                network_allowed: true,
            }
        }
        fn pair(&self) {
            record_original(
                Path::new(&self.result.url).parent().unwrap(),
                "001.jpg",
                &self.original,
            )
            .unwrap();
        }
        fn preview(&self) -> PhotoPreview {
            let prepared =
                prepare(&self.context(), &self.result.id, &self.result.source_id).unwrap();
            let cache = self.temp.path().join("cache/media");
            fs::create_dir_all(&cache).unwrap();
            prepared.publish(&cache).unwrap()
        }
    }
    fn save(path: &Path, w: u32, h: u32) {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(w, h)
            .write_to(
                &mut bytes,
                if path.extension().unwrap() == "jpg" {
                    ImageFormat::Jpeg
                } else {
                    ImageFormat::Png
                },
            )
            .unwrap();
        fs::write(path, bytes.into_inner()).unwrap();
    }
    #[test]
    fn full_saved_preview_preserves_4k_pixels_and_exact_bytes() {
        let f = Fixture::new();
        let response = f.preview();
        assert_eq!((response.image.width, response.image.height), (3840, 2160));
        assert_eq!(
            fs::read(&response.image.url).unwrap(),
            fs::read(&f.result.url).unwrap()
        );
        assert!(Path::new(&response.image.url).starts_with(f.temp.path().join("cache/media")));
    }
    #[test]
    fn pair_survives_reopen_with_both_sources_disabled_without_enabling_them() {
        let mut f = Fixture::new();
        f.pair();
        f.sources = serde_json::from_slice(&serde_json::to_vec(&f.sources).unwrap()).unwrap();
        let preview = f.preview();
        assert_eq!(
            (
                preview.before.as_ref().unwrap().width,
                preview.before.unwrap().height
            ),
            (960, 540)
        );
        assert!(!preview.comparison_unavailable);
        assert!(f.sources.iter().all(|s| !s.enabled));
        let metadata = fs::read_to_string(
            Path::new(&f.result.url)
                .parent()
                .unwrap()
                .join("comparisons.json"),
        )
        .unwrap();
        assert!(metadata.contains(&f.original.id));
        assert!(!metadata.contains("original.png"));
        assert!(!metadata.contains("password"));
    }
    #[test]
    fn legacy_and_missing_original_keep_result_preview_without_guessing_pairs() {
        let f = Fixture::new();
        let legacy = f.preview();
        assert!(legacy.before.is_none());
        assert!(legacy.comparison_unavailable);
        f.pair();
        fs::remove_file(&f.original.url).unwrap();
        let missing = f.preview();
        assert!(missing.before.is_none());
        assert!(missing.comparison_unavailable);
        assert_eq!((missing.image.width, missing.image.height), (3840, 2160));
    }
    #[test]
    fn ordinary_original_is_preview_only_without_unavailable_warning() {
        let f = Fixture::new();
        let prepared = prepare(&f.context(), &f.original.id, &f.original.source_id).unwrap();
        assert!(prepared.before.is_none());
        assert!(!prepared.comparison_unavailable);
    }
    #[test]
    fn forged_ids_paths_and_wrong_source_are_denied() {
        let f = Fixture::new();
        for id in [
            "../original.png",
            f.original.url.as_str(),
            f.original.id.as_str(),
        ] {
            assert!(prepare(&f.context(), id, &f.result.source_id).is_err());
        }
        assert!(prepare(&f.context(), &f.result.id, "unknown").is_err());
    }
    #[test]
    fn large_original_is_bounded_and_reports_served_dimensions() {
        let f = Fixture::new();
        save(Path::new(&f.original.url), 4000, 1000);
        f.pair();
        let before = f.preview().before.unwrap();
        assert_eq!((before.width, before.height), (3840, 960));
        assert_eq!(image::image_dimensions(before.url).unwrap(), (3840, 960));
    }
    #[test]
    fn animated_formats_are_served_as_bounded_static_images() {
        let f = Fixture::new();
        let mut gif = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(20, 10)
            .write_to(&mut gif, ImageFormat::Gif)
            .unwrap();
        fs::write(&f.original.url, gif.into_inner()).unwrap();
        let prepared = prepare(&f.context(), &f.original.id, &f.original.source_id).unwrap();
        assert!(matches!(
            image::guess_format(&prepared.image.bytes).unwrap(),
            ImageFormat::Png | ImageFormat::Jpeg
        ));
    }
    #[test]
    fn clearing_preview_cache_does_not_remove_durable_mapping() {
        let f = Fixture::new();
        f.pair();
        let first = f.preview();
        fs::remove_dir_all(f.temp.path().join("cache")).unwrap();
        assert!(!Path::new(&first.image.url).exists());
        assert!(f.preview().before.is_some());
    }
    #[test]
    fn association_filenames_cannot_escape_collection() {
        let f = Fixture::new();
        let collection = Path::new(&f.result.url).parent().unwrap();
        assert!(record_original(collection, "../outside.jpg", &f.original).is_err());
    }
}
