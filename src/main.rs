use anyhow::Result;
use clap::{Parser, Subcommand};
use glam::Vec2;
use rusty_spine::{
    atlas::{AtlasFilter, AtlasFormat, AtlasWrap},
    Atlas, SkeletonJson, Skin,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
mod spine;
use miniquad::*;
use spine::{Render, Spine, SpineInfo, SpineSkeletonPath, SpineTexture};

// 1. Struct globale du CLI
#[derive(Parser, Debug)]
#[command(author, version, about = "Spine composite renderer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// 2. Sous-commandes
#[derive(Subcommand, Debug)]
enum Commands {
    /// Génère une image PNG à partir d'un JSON Spine et d'un atlas
    Render {
        /// Chemin vers le fichier Skeleton JSON Spine
        #[arg(long, value_name = "FILE")]
        json: PathBuf,

        /// Chemin vers le fichier atlas Spine (.atlas)
        #[arg(long, value_name = "FILE")]
        atlas: PathBuf,

        /// Chemin de sortie pour le PNG généré
        #[arg(long, value_name = "FILE", default_value = "out.png")]
        out: PathBuf,

        /// Skin de base (ex. "broccoli")
        #[arg(long, default_value = "BASES/Broccoli_Base")]
        base_skin: String,

        /// Liste de skins additionnels à fusionner (séparés par virgule)
        #[arg(long, value_delimiter = ',')]
        skins: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse(); // :contentReference[oaicite:2]{index=2}

    match cli.command {
        Commands::Render {
            json,
            atlas,
            out,
            base_skin,
            skins,
        } => {
            render(&json, &atlas, &out, &base_skin, &skins)?;
        }
    }
    Ok(())
}

pub fn render(
    json_path: &Path,
    atlas_path: &Path,
    output_path: &Path,
    base_skin: &str,
    skins_to_add: &[String],
) -> anyhow::Result<()> {
    // These texture callbacks should be set before loading an atlas.
    rusty_spine::extension::set_create_texture_cb(|atlas_page, path| {
        fn convert_filter(filter: AtlasFilter) -> FilterMode {
            match filter {
                AtlasFilter::Linear => FilterMode::Linear,
                AtlasFilter::Nearest => FilterMode::Nearest,
                filter => {
                    println!("Unsupported texture filter mode: {filter:?}");
                    FilterMode::Linear
                }
            }
        }
        fn convert_wrap(wrap: AtlasWrap) -> TextureWrap {
            match wrap {
                AtlasWrap::ClampToEdge => TextureWrap::Clamp,
                AtlasWrap::MirroredRepeat => TextureWrap::Mirror,
                AtlasWrap::Repeat => TextureWrap::Repeat,
                wrap => {
                    println!("Unsupported texture wrap mode: {wrap:?}");
                    TextureWrap::Clamp
                }
            }
        }
        fn convert_format(format: AtlasFormat) -> TextureFormat {
            match format {
                AtlasFormat::RGB888 => TextureFormat::RGB8,
                AtlasFormat::RGBA8888 => TextureFormat::RGBA8,
                format => {
                    println!("Unsupported texture format: {format:?}");
                    TextureFormat::RGBA8
                }
            }
        }
        atlas_page
            .renderer_object()
            .set(SpineTexture::NeedsToBeLoaded {
                path: path.to_owned(),
                min_filter: convert_filter(atlas_page.min_filter()),
                mag_filter: convert_filter(atlas_page.mag_filter()),
                x_wrap: convert_wrap(atlas_page.u_wrap()),
                y_wrap: convert_wrap(atlas_page.v_wrap()),
                format: convert_format(atlas_page.format()),
            });
    });


    // Charger l’atlas Spine
    let atlas = Arc::new(Atlas::new_from_file(atlas_path)?);

    // Lire le JSON de squelette
    let skeleton_json = SkeletonJson::new(atlas.clone());
    let skeleton_data = Arc::new(skeleton_json.read_skeleton_data_file(json_path)?);

    // Composer le skin
    let mut composite = skeleton_data
        .find_skin(base_skin)
        .expect("Base skin not found")
        .clone();
    for skin in skins_to_add {
        let s = skeleton_data
            .find_skin(skin)
            .expect("Additional skin not found");
        unsafe { composite.add_skin(&s) };
    }
    let composite_static: &'static Skin = Box::leak(Box::new(composite));

    let atlas_path_string = atlas_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid atlas path"))?
        .to_owned()
        .into_boxed_str();
    // on fuit la Box<str> pour obtenir &'static str
    let atlas_path_static: &'static str = Box::leak(atlas_path_string);

    let skeleton_path_string = json_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid skeleton path"))?
        .to_owned()
        .into_boxed_str();
    let skeleton_path_static: &'static str = Box::leak(skeleton_path_string);


    let conf = conf::Conf {
        window_title: "rusty_spine".to_owned(),
        high_dpi: true,
        ..Default::default()
    };

    let texture_delete_queue: Arc<Mutex<Vec<Texture>>> = Arc::new(Mutex::new(vec![]));
    let texture_delete_queue_cb = texture_delete_queue.clone();
    rusty_spine::extension::set_dispose_texture_cb(move |atlas_page| unsafe {
        if let Some(SpineTexture::Loaded(texture)) =
            atlas_page.renderer_object().get::<SpineTexture>()
        {
            texture_delete_queue_cb.lock().unwrap().push(*texture);
        }
        atlas_page.renderer_object().dispose::<SpineTexture>();
    });

    println!("Atlas path: {}", atlas_path.display());
    println!("Skeleton path: {}", json_path.display());
    println!("Output path: {}", output_path.display());
    println!("Base skin: {}", base_skin);
    println!("Additional skins: {:?}", skins_to_add);
    println!("Output path: {}", output_path.display());

    // 2) Passe les références &atlas_path_str et &skeleton_path_str
    let spine_info = SpineInfo {
        atlas_path: atlas_path_static,
        skeleton_path: SpineSkeletonPath::Json(skeleton_path_static),
        animation: "Idle_Happy",
        position: Vec2::ZERO,
        scale: 1.0,
        skin: Some(&composite_static),
        backface_culling: false,
    };
    let spine_info_static: &'static SpineInfo = Box::leak(Box::new(spine_info));

    miniquad::start(conf, |ctx| {
        Box::new(Render::new(ctx, texture_delete_queue, spine_info_static))
    });

    Ok(())
}
