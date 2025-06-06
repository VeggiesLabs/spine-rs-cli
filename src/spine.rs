use glam::{Mat4, Vec2};
use image::{ImageBuffer, Rgba};
use miniquad::*;
use miniquad::{FilterMode, RenderPass, TextureFormat, TextureParams, TextureWrap};
use rusty_spine::{
    controller::{SkeletonController, SkeletonControllerSettings},
    draw::{ColorSpace, CullDirection},
    AnimationEvent, AnimationStateData, Atlas, BlendMode, Color, Physics, SkeletonBinary,
    SkeletonJson, Skin,
};
use std::process::exit;
use std::sync::{Arc, Mutex};

const MAX_MESH_VERTICES: usize = 10000;
const MAX_MESH_INDICES: usize = 5000;

/// Holds all data related to load and demonstrate a particular Spine skeleton.
#[derive(Clone, Copy, Debug)]
pub struct SpineInfo {
    pub atlas_path: &'static str,
    pub skeleton_path: SpineSkeletonPath,
    pub animation: &'static str,
    pub position: Vec2,
    pub scale: f32,
    pub skin: Option<&'static Skin>,
    pub backface_culling: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum SpineSkeletonPath {
    Binary(&'static str),
    Json(&'static str),
}

pub struct Spine {
    controller: SkeletonController,
    world: Mat4,
    cull_face: CullFace,
}

impl Spine {
    pub fn load(info: SpineInfo) -> Self {
        // Load atlas and auto-detect if the textures are premultiplied
        let atlas = Arc::new(
            Atlas::new_from_file(info.atlas_path)
                .unwrap_or_else(|_| panic!("failed to load atlas file: {}", info.atlas_path)),
        );
        let premultiplied_alpha = atlas.pages().any(|page| page.pma());

        // Load either binary or json skeleton files
        let skeleton_data = Arc::new(match info.skeleton_path {
            SpineSkeletonPath::Binary(path) => {
                let skeleton_binary = SkeletonBinary::new(atlas);
                skeleton_binary
                    .read_skeleton_data_file(path)
                    .unwrap_or_else(|_| panic!("failed to load binary skeleton file: {path}"))
            }
            SpineSkeletonPath::Json(path) => {
                let skeleton_json = SkeletonJson::new(atlas);
                skeleton_json
                    .read_skeleton_data_file(path)
                    .unwrap_or_else(|_| panic!("failed to load json skeleton file: {path}"))
            }
        });

        // Create animation state data from a skeleton
        // If desired, set crossfades at this point
        // See [`rusty_spine::AnimationStateData::set_mix_by_name`]
        let animation_state_data = Arc::new(AnimationStateData::new(skeleton_data.clone()));

        // Instantiate the [`rusty_spine::controller::SkeletonController`] helper class which
        // handles creating the live data ([`rusty_spine::Skeleton`] and
        // [`rusty_spine::AnimationState`] and capable of generating mesh render data.
        // Use of this helper is not required but it does handle a lot of little things for you.
        let mut controller = SkeletonController::new(skeleton_data, animation_state_data)
            .with_settings(SkeletonControllerSettings {
                premultiplied_alpha,
                cull_direction: CullDirection::CounterClockwise,
                color_space: ColorSpace::SRGB,
            });

        // Listen for animation events
        controller
            .animation_state
            .set_listener(|_, animation_event| match animation_event {
                AnimationEvent::Start { track_entry } => {
                    println!("Animation {} started!", track_entry.track_index());
                }
                AnimationEvent::Interrupt { track_entry } => {
                    println!("Animation {} interrupted!", track_entry.track_index());
                }
                AnimationEvent::End { track_entry } => {
                    println!("Animation {} ended!", track_entry.track_index());
                }
                AnimationEvent::Complete { track_entry } => {
                    println!("Animation {} completed!", track_entry.track_index());
                }
                AnimationEvent::Dispose { track_entry } => {
                    println!("Animation {} disposed!", track_entry.track_index());
                }
                AnimationEvent::Event {
                    track_entry,
                    name,
                    int,
                    float,
                    string,
                    audio_path,
                    volume,
                    balance,
                    ..
                } => {
                    println!("Animation {} event!", track_entry.track_index());
                    println!("  Name: {name}");
                    println!("  Integer: {int}");
                    println!("  Float: {float}");
                    if !string.is_empty() {
                        println!("  String: \"{string}\"");
                    }
                    if !audio_path.is_empty() {
                        println!("  Audio: \"{audio_path}\"");
                        println!("    Volume: {volume}");
                        println!("    Balance: {balance}");
                    }
                }
            });

        // Start the animation on track 0 and loop
        controller
            .animation_state
            .set_animation_by_name(0, info.animation, true)
            .unwrap_or_else(|_| panic!("failed to start animation: {}", info.animation));

        // If a skin was provided, set it
        if let Some(skin) = info.skin {
            unsafe { controller.skeleton.set_skin(skin) }
        }

        controller.settings.premultiplied_alpha = premultiplied_alpha;
        let mut pos = info.position;
        pos.y -= 300.0;
        println!("Position: {:?}", pos);
        Self {
            controller,
            world: Mat4::from_translation(pos.extend(0.))
                * Mat4::from_scale(Vec2::splat(info.scale).extend(1.)),
            cull_face: match info.backface_culling {
                false => CullFace::Nothing,
                true => CullFace::Back,
            },
        }
    }
}

pub struct Stage {
    spine: Spine,
    pipeline: Pipeline,
    bindings: Vec<Bindings>,
    texture_delete_queue: Arc<Mutex<Vec<Texture>>>,
    last_frame_time: f64,
    screen_size: Vec2,
    render_png: bool,
    render_pass: RenderPass,
    color_texture: Texture,
    png_path: String,
}

impl Stage {
    pub fn new(
        ctx: &mut Context,
        texture_delete_queue: Arc<Mutex<Vec<Texture>>>,
        spine_info: &SpineInfo,
        render_png: &bool,
        png_path: &str,
    ) -> Stage {
        // --- création de la texture et du render pass ---
        let (w_px, h_px) = (
            (800. * ctx.dpi_scale()) as u32,
            (800. * ctx.dpi_scale()) as u32,
        );
        let color_tex = Texture::new_render_texture(
            ctx,
            TextureParams {
                format: TextureFormat::RGBA8,
                wrap: TextureWrap::Clamp,
                filter: FilterMode::Nearest,
                width: w_px,
                height: h_px,
                ..Default::default()
            },
        );
        let depth_tex = Texture::new_render_texture(
            ctx,
            TextureParams {
                format: TextureFormat::Depth,
                height: h_px,
                width: w_px,
                ..Default::default()
            },
        );
        let render_pass = RenderPass::new(ctx, color_tex, Some(depth_tex));

        let spine_info = *spine_info;
        let spine = Spine::load(spine_info);

        Stage {
            spine,
            pipeline: create_pipeline(ctx),
            bindings: vec![],
            texture_delete_queue,
            last_frame_time: date::now(),
            screen_size: Vec2::new(w_px as f32, h_px as f32),
            render_png: *render_png,
            render_pass,
            color_texture: color_tex,
            png_path: png_path.to_string(),
        }
    }

    fn view(&self) -> Mat4 {
        Mat4::orthographic_rh_gl(
            self.screen_size.x * -0.5,
            self.screen_size.x * 0.5,
            self.screen_size.y * -0.5,
            self.screen_size.y * 0.5,
            0.,
            1.,
        )
    }
}

impl EventHandler for Stage {
    fn update(&mut self, _ctx: &mut Context) {
        let now = date::now();
        let dt = ((now - self.last_frame_time) as f32).max(0.001);
        self.spine.controller.update(dt, Physics::Update);
        self.last_frame_time = now;
    }

    fn draw(&mut self, ctx: &mut Context) {
        let renderables = self.spine.controller.combined_renderables();

        // Create bindings that can be re-used for rendering Spine meshes
        while renderables.len() > self.bindings.len() {
            let vertex_buffer = Buffer::stream(
                ctx,
                BufferType::VertexBuffer,
                MAX_MESH_VERTICES * std::mem::size_of::<Vertex>(),
            );
            let index_buffer = Buffer::stream(
                ctx,
                BufferType::IndexBuffer,
                MAX_MESH_INDICES * std::mem::size_of::<u16>(),
            );
            self.bindings.push(Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![Texture::empty()],
            });
        }

        if self.render_png {
            // --- on dessine dans notre RenderPass au lieu du default backbuffer ---
            let pass_action = miniquad::PassAction::clear_color(0.1, 0.1, 0.1, 0.0);
            ctx.begin_pass(self.render_pass, pass_action);
            ctx.apply_pipeline(&self.pipeline);
            ctx.set_cull_face(self.spine.cull_face);

            let view = self.view();
            for (renderable, bindings) in renderables.into_iter().zip(self.bindings.iter_mut()) {
                let BlendStates {
                    alpha_blend,
                    color_blend,
                } = renderable
                    .blend_mode
                    .get_blend_states(self.spine.controller.settings.premultiplied_alpha);
                ctx.set_blend(Some(color_blend), Some(alpha_blend));
                // Préparation des vertex/index buffers et des textures
                let mut vertices = Vec::with_capacity(renderable.vertices.len());
                for i in 0..renderable.vertices.len() {
                    vertices.push(Vertex {
                        position: Vec2 {
                            x: renderable.vertices[i][0],
                            y: renderable.vertices[i][1],
                        },
                        uv: Vec2 {
                            x: renderable.uvs[i][0],
                            y: renderable.uvs[i][1],
                        },
                        color: Color::from(renderable.colors[i]),
                        dark_color: Color::from(renderable.dark_colors[i]),
                    });
                }
                bindings.vertex_buffers[0].update(ctx, &vertices);
                bindings.index_buffer.update(ctx, &renderable.indices);

                // Si pas d'attachement (pas de texture), on skip ce renderable
                let Some(attachment_renderer_object) = renderable.attachment_renderer_object else {
                    continue;
                };
                let spine_texture =
                    unsafe { &mut *(attachment_renderer_object as *mut SpineTexture) };
                let texture = match spine_texture {
                    SpineTexture::NeedsToBeLoaded {
                        path,
                        min_filter,
                        mag_filter,
                        x_wrap,
                        y_wrap,
                        format,
                    } => {
                        // on emprunte path au lieu de le déplacer
                        let image = image::io::Reader::open(&*path)
                            .unwrap_or_else(|_| panic!("failed to open image: {}", path))
                            .decode()
                            .unwrap_or_else(|_| panic!("failed to decode image: {}", path));
                        // création de la texture GPU
                        let params = TextureParams {
                            width: image.width(),
                            height: image.height(),
                            format: *format,
                            ..Default::default()
                        };
                        let tex = match format {
                            TextureFormat::RGB8 => {
                                Texture::from_data_and_format(ctx, &image.to_rgb8(), params)
                            }
                            TextureFormat::RGBA8 => {
                                Texture::from_data_and_format(ctx, &image.to_rgba8(), params)
                            }
                            _ => unreachable!(),
                        };
                        tex.set_filter_min_mag(ctx, *min_filter, *mag_filter);
                        tex.set_wrap_xy(ctx, *x_wrap, *y_wrap);
                        // on remplace l'état pour ne plus recharger la prochaine frame
                        *spine_texture = SpineTexture::Loaded(tex);
                        // et on en extrait le handle (Texture est Copy)
                        if let SpineTexture::Loaded(t) = spine_texture {
                            *t
                        } else {
                            unreachable!()
                        }
                    }
                    SpineTexture::Loaded(t) => *t,
                };
                bindings.images = vec![texture];

                ctx.apply_bindings(bindings);
                ctx.apply_uniforms(&shader::Uniforms {
                    world: self.spine.world,
                    view,
                });
                ctx.draw(0, renderable.indices.len() as i32, 1);
            }

            ctx.end_render_pass();
            ctx.commit_frame();

            // Lit le pixel buffer RGBA depuis la texture
            // width and height in *pixels* (after applying DPI scale)
            let w = (self.screen_size.x * ctx.dpi_scale()) as usize;
            let h = (self.screen_size.y * ctx.dpi_scale()) as usize;

            // allocate a buffer for RGBA8 bytes
            let mut pixels = vec![0u8; w * h * 4];

            // actually read them back from the GPU
            self.color_texture.read_pixels(&mut pixels);

            // flip vertically in-place
            // flip vertically in-place
            let line_bytes = w * 4;
            for y in 0..(h / 2) {
                let top_start = y * line_bytes;
                let bot_start = (h - 1 - y) * line_bytes;

                // split into [0 .. bot_start) and [bot_start .. end);
                // these two slices are guaranteed non-overlapping
                let (prefix, suffix) = pixels.split_at_mut(bot_start);

                // within prefix, take the top row
                let top_row = &mut prefix[top_start..top_start + line_bytes];
                // within suffix, take the bottom row (which is at the start of suffix)
                let bottom_row = &mut suffix[..line_bytes];

                // now swap them
                top_row.swap_with_slice(bottom_row);
            }

            // then build your ImageBuffer and save as before
            let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(w as u32, h as u32, pixels)
                .expect("Erreur création ImageBuffer");
            img.save(self.png_path.clone())
                .expect("Impossible d’enregistrer frame0.png");
            println!("✅ Première frame écrite dans {}", self.png_path);

            exit(0);
        } else {
            // Delete textures that are no longer used. The delete call needs to happen here, before
            // rendering, or it may not actually delete the texture.
            for texture_delete in self.texture_delete_queue.lock().unwrap().drain(..) {
                texture_delete.delete();
            }

            // Begin frame
            ctx.begin_default_pass(Default::default());
            ctx.clear(Some((0.1, 0.1, 0.1, 0.0)), None, None);
            ctx.apply_pipeline(&self.pipeline);

            // Apply backface culling only if this skeleton needs it
            ctx.set_cull_face(self.spine.cull_face);

            let view = self.view();
            for (renderable, bindings) in renderables.into_iter().zip(self.bindings.iter_mut()) {
                // Set blend state based on this renderable's blend mode
                let BlendStates {
                    alpha_blend,
                    color_blend,
                } = renderable
                    .blend_mode
                    .get_blend_states(self.spine.controller.settings.premultiplied_alpha);
                ctx.set_blend(Some(color_blend), Some(alpha_blend));

                // Create the vertex and index buffers for miniquad
                let mut vertices = vec![];
                for vertex_index in 0..renderable.vertices.len() {
                    vertices.push(Vertex {
                        position: Vec2 {
                            x: renderable.vertices[vertex_index][0],
                            y: renderable.vertices[vertex_index][1],
                        },
                        uv: Vec2 {
                            x: renderable.uvs[vertex_index][0],
                            y: renderable.uvs[vertex_index][1],
                        },
                        color: Color::from(renderable.colors[vertex_index]),
                        dark_color: Color::from(renderable.dark_colors[vertex_index]),
                    });
                }
                bindings.vertex_buffers[0].update(ctx, &vertices);
                bindings.index_buffer.update(ctx, &renderable.indices);

                // If there is no attachment (and therefore no texture), skip rendering this renderable
                // May also be None if a create texture callback was never set.
                let Some(attachment_renderer_object) = renderable.attachment_renderer_object else {
                    continue;
                };

                // Load textures if they haven't been loaded already
                let spine_texture =
                    unsafe { &mut *(attachment_renderer_object as *mut SpineTexture) };
                let texture = match spine_texture {
                    SpineTexture::NeedsToBeLoaded {
                        path,
                        min_filter,
                        mag_filter,
                        x_wrap,
                        y_wrap,
                        format,
                    } => {
                        use image::io::Reader as ImageReader;

                        #[allow(clippy::needless_borrows_for_generic_args)]
                        let image = ImageReader::open(&path)
                            .unwrap_or_else(|_| panic!("failed to open image: {}", &path))
                            .decode()
                            .unwrap_or_else(|_| panic!("failed to decode image: {}", &path));
                        let texture_params = TextureParams {
                            width: image.width(),
                            height: image.height(),
                            format: *format,
                            ..Default::default()
                        };
                        let texture = match format {
                            TextureFormat::RGB8 => {
                                Texture::from_data_and_format(ctx, &image.to_rgb8(), texture_params)
                            }
                            TextureFormat::RGBA8 => Texture::from_data_and_format(
                                ctx,
                                &image.to_rgba8(),
                                texture_params,
                            ),
                            _ => unreachable!(),
                        };
                        texture.set_filter_min_mag(ctx, *min_filter, *mag_filter);
                        texture.set_wrap_xy(ctx, *x_wrap, *y_wrap);
                        *spine_texture = SpineTexture::Loaded(texture);
                        texture
                    }
                    SpineTexture::Loaded(texture) => *texture,
                };
                bindings.images = vec![texture];

                // Draw this renderable
                ctx.apply_bindings(bindings);
                ctx.apply_uniforms(&shader::Uniforms {
                    world: self.spine.world,
                    view,
                });
                ctx.draw(0, renderable.indices.len() as i32, 1);
            }

            // End frame
            ctx.end_render_pass();
            ctx.commit_frame();
        }
    }

    fn resize_event(&mut self, ctx: &mut Context, width: f32, height: f32) {
        self.screen_size = Vec2::new(width, height) / ctx.dpi_scale();
    }
}

fn create_pipeline(ctx: &mut Context) -> Pipeline {
    let shader = Shader::new(ctx, shader::VERTEX, shader::FRAGMENT, shader::meta())
        .expect("failed to build shader");
    Pipeline::new(
        ctx,
        &[BufferLayout::default()],
        &[
            VertexAttribute::new("position", VertexFormat::Float2),
            VertexAttribute::new("uv", VertexFormat::Float2),
            VertexAttribute::new("color", VertexFormat::Float4),
            VertexAttribute::new("dark_color", VertexFormat::Float4),
        ],
        shader,
    )
}

#[repr(C)]
struct Vertex {
    position: Vec2,
    uv: Vec2,
    color: Color,
    dark_color: Color,
}
struct BlendStates {
    alpha_blend: BlendState,
    color_blend: BlendState,
}

trait GetBlendStates {
    fn get_blend_states(&self, premultiplied_alpha: bool) -> BlendStates;
}

impl GetBlendStates for BlendMode {
    fn get_blend_states(&self, premultiplied_alpha: bool) -> BlendStates {
        match self {
            Self::Additive => match premultiplied_alpha {
                // Case 1: Additive Blend Mode, Normal Alpha
                false => BlendStates {
                    alpha_blend: BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::One),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::One,
                    ),
                },
                // Case 2: Additive Blend Mode, Premultiplied Alpha
                true => BlendStates {
                    alpha_blend: BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::One),
                    color_blend: BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::One),
                },
            },
            Self::Multiply => match premultiplied_alpha {
                // Case 3: Multiply Blend Mode, Normal Alpha
                false => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::DestinationColor),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
                // Case 4: Multiply Blend Mode, Premultiplied Alpha
                true => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::DestinationColor),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
            },
            Self::Normal => match premultiplied_alpha {
                // Case 5: Normal Blend Mode, Normal Alpha
                false => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
                // Case 6: Normal Blend Mode, Premultiplied Alpha
                true => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
            },
            Self::Screen => match premultiplied_alpha {
                // Case 7: Screen Blend Mode, Normal Alpha
                false => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::OneMinusValue(BlendValue::SourceColor),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
                // Case 8: Screen Blend Mode, Premultiplied Alpha
                true => BlendStates {
                    alpha_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::OneMinusValue(BlendValue::SourceColor),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                    color_blend: BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    ),
                },
            },
        }
    }
}

#[derive(Debug)]
pub enum SpineTexture {
    NeedsToBeLoaded {
        path: String,
        min_filter: FilterMode,
        mag_filter: FilterMode,
        x_wrap: TextureWrap,
        y_wrap: TextureWrap,
        format: TextureFormat,
    },
    Loaded(Texture),
}
mod shader {
    use glam::Mat4;
    use miniquad::*;

    pub const VERTEX: &str = r#"
        #version 100
        attribute vec2 position;
        attribute vec2 uv;
        attribute vec4 color;
        attribute vec4 dark_color;

        uniform mat4 world;
        uniform mat4 view;

        varying lowp vec2 f_texcoord;
        varying lowp vec4 f_color;
        varying lowp vec4 f_dark_color;

        void main() {
            gl_Position = view * world * vec4(position, 0, 1);
            f_texcoord = uv;
            f_color = color;
            f_dark_color = dark_color;
        }
    "#;

    pub const FRAGMENT: &str = r#"
        #version 100
        varying lowp vec2 f_texcoord;
        varying lowp vec4 f_color;
        varying lowp vec4 f_dark_color;

        uniform sampler2D tex;

        void main() {
            lowp vec4 tex_color = texture2D(tex, f_texcoord);
            gl_FragColor = vec4(
                ((tex_color.a - 1.0) * f_dark_color.a + 1.0 - tex_color.rgb) * f_dark_color.rgb + tex_color.rgb * f_color.rgb,
                tex_color.a * f_color.a
            );
        }
    "#;

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["tex".to_string()],
            uniforms: UniformBlockLayout {
                uniforms: vec![
                    UniformDesc::new("world", UniformType::Mat4),
                    UniformDesc::new("view", UniformType::Mat4),
                ],
            },
        }
    }

    #[repr(C)]
    pub struct Uniforms {
        pub world: Mat4,
        pub view: Mat4,
    }
}
