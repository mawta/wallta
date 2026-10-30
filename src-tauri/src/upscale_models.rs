//! The only allowed runtime/model assets. Names, sizes and SHA-256 come from
//! the verified official 20220424 Windows archive.
#[derive(Clone, Copy, Debug)]
pub(super) struct Asset {
    pub path: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub runtime_name: &'static str,
    pub files: &'static [Asset],
}

pub(super) const RUNTIME: &[Asset] = &[
    Asset {
        path: "realesrgan-ncnn-vulkan.exe",
        bytes: 6161408,
        sha256: "07e49f7cbb4ede01ae4dd4c399d3a7e5846e3d2085c3128eff881e55cb7b1a0c",
    },
    Asset {
        path: "vcomp140.dll",
        bytes: 182704,
        sha256: "8f72ef2e483465444b2059fc6744d6cb22cd8d8a27f6fa56befd2a42dcd0f78b",
    },
];
pub(super) const MODELS: &[Model] = &[
    Model {
        id: "photo",
        name: "Photo",
        description: "Detailed photographs and general images.",
        runtime_name: "realesrgan-x4plus",
        files: &[
            Asset {
                path: "models/realesrgan-x4plus.bin",
                bytes: 33424520,
                sha256: "713ee713b0353afaa27976f0563a64a5043bd70b9bd8936c2e26e25ebcdbcddf",
            },
            Asset {
                path: "models/realesrgan-x4plus.param",
                bytes: 116029,
                sha256: "35330ececcea33b6c397a72548e788d5d53becee4734c50b7fada36e89f10a86",
            },
        ],
    },
    Model {
        id: "anime",
        name: "Anime",
        description: "Illustrations and anime artwork.",
        runtime_name: "realesrgan-x4plus-anime",
        files: &[
            Asset {
                path: "models/realesrgan-x4plus-anime.bin",
                bytes: 8943500,
                sha256: "fe01c269cfd10cdef8e018ab66ebe750cf79c7af4d1f9c16c737e1295229bacc",
            },
            Asset {
                path: "models/realesrgan-x4plus-anime.param",
                bytes: 30290,
                sha256: "2b8fb6e0ae4d2d85704ca08c119a2f5ea40add4f2ecd512eb7f4cd44b6127ed4",
            },
        ],
    },
    Model {
        id: "anime-light",
        name: "Anime Light",
        description: "Small, fast model for anime and illustrations.",
        runtime_name: "realesr-animevideov3",
        files: &[
            Asset {
                path: "models/realesr-animevideov3-x4.bin",
                bytes: 1247368,
                sha256: "548a36f9c3f4ab8da56cd3b13badf23968bee207b396dad14d04b830e5f2ab2d",
            },
            Asset {
                path: "models/realesr-animevideov3-x4.param",
                bytes: 3077,
                sha256: "850a248e7c14c27e5bd8cf7265113a9441036a7db63963bb8aa5169d788a435e",
            },
        ],
    },
];

pub(super) fn selected(id: Option<&str>) -> Result<&'static Model, String> {
    let id = id.unwrap_or("photo");
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| "Unknown AI model. Choose Photo, Anime or Anime Light.".into())
}

pub(super) fn missing_assets(
    model: &'static Model,
    installed: impl Fn(&Asset) -> bool,
) -> Vec<&'static Asset> {
    RUNTIME
        .iter()
        .chain(model.files.iter())
        .filter(|asset| !installed(asset))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_models_have_separate_exact_download_sizes() {
        assert_eq!(RUNTIME.iter().map(|a| a.bytes).sum::<u64>(), 6_344_112);
        for (id, size) in [
            ("photo", 33_540_549),
            ("anime", 8_973_790),
            ("anime-light", 1_250_445),
        ] {
            let model = selected(Some(id)).unwrap();
            assert_eq!(model.files.len(), 2);
            assert_eq!(model.files.iter().map(|a| a.bytes).sum::<u64>(), size);
            assert!(model.files.iter().all(|a| a.sha256.len() == 64));
        }
    }

    #[test]
    fn model_selection_defaults_to_photo_and_rejects_paths() {
        assert_eq!(selected(None).unwrap().id, "photo");
        assert_eq!(
            selected(Some("anime-light")).unwrap().runtime_name,
            "realesr-animevideov3"
        );
        for input in ["../photo", "C:\\model", "realesrgan-x4plus", "", "PHOTO"] {
            assert!(selected(Some(input)).is_err(), "{input}");
        }
    }

    #[test]
    fn download_plan_contains_only_selected_pair_and_missing_shared_runtime() {
        let light = selected(Some("anime-light")).unwrap();
        let empty = missing_assets(light, |_| false);
        assert_eq!(empty.len(), 4);
        assert_eq!(empty.iter().map(|a| a.bytes).sum::<u64>(), 7_594_557);
        assert!(empty.iter().all(|a| !a.path.contains("x4plus")));
        let runtime_ready = missing_assets(light, |a| !a.path.starts_with("models/"));
        assert_eq!(runtime_ready.len(), 2);
        assert_eq!(
            runtime_ready.iter().map(|a| a.bytes).sum::<u64>(),
            1_250_445
        );
        assert!(missing_assets(light, |_| true).is_empty());
    }
}
