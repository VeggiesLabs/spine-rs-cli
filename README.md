# spine-rs-cli

A lightweight command-line interface tool for rendering Spine animations to PNG images using Rust and the `rusty_spine` library.

## Overview

`spine-rs-cli` provides a simple workflow to load Spine skeleton data (JSON or binary) along with its associated texture atlas, compose skins, and render a specified animation frame (or looping animation) directly to a PNG image. It's ideal for automated asset generation, batch rendering, or integrating Spine rendering into Rust-based toolchains.

## Architecture

- **CLI Entry Point (`main.rs`)**
  - Uses `clap` for argument parsing and subcommands.
  - Defines a `Render` subcommand for PNG generation.
  - Sets up texture callbacks and initializes `miniquad` window to drive the rendering loop.

- **Rendering Module (`spine.rs`)**
  - **`SpineInfo`**: Configuration struct holding paths, animation name, position, scale, skin, and culling options.
  - **`Spine`**: Handles loading the atlas, skeleton data (JSON or binary), creating the `SkeletonController`, and configuring animation playback.
  - **`Render`**: Implements `miniquad::EventHandler` to update and draw the skeleton each frame, manage GPU buffers, and handle texture loading/disposal.
  - Blend mode support, premultiplied alpha detection, backface culling, and animation event logging.

## Features

- Render Spine JSON or binary skeletons.
- Composite multiple skins onto a base skin.
- Loop animations or render a single frame.
- Configurable position, scale, and backface culling.
- Automatic premultiplied alpha handling and blend mode support.
- Lightweight dependency on `rusty_spine`, `miniquad`, and `glam`.

## Installation

Ensure you have Rust and Cargo installed. Then:

```bash
git clone https://github.com/yourusername/spine-rs-cli.git
cd spine-rs-cli
cargo build --release
```

The compiled binary will be available at `target/release/spine-rs-cli`.

## Usage

### Render Subcommand

Generate a PNG from a Spine JSON skeleton and atlas:

```bash
spine-rs-cli render \
  --json path/to/skeleton.json \
  --atlas path/to/skeleton.atlas \
  --out output.png \
  --base-skin "BASES/Broccoli_Base" \
  --skins "Skin1,Skin2"
```

- `--json <FILE>`: Path to the Spine skeleton JSON file.
- `--atlas <FILE>`: Path to the Spine atlas file (.atlas).
- `--out <FILE>`: Output path for the generated PNG (default: `out.png`).
- `--base-skin <NAME>`: Name of the base skin in the skeleton data.
- `--skins <LIST>`: Comma-separated list of additional skin names to composite.

### Examples

- **Basic render**:

  ```bash
  spine-rs-cli render --json hero.json --atlas hero.atlas --out hero.png
  ```

- **With custom skins**:

  ```bash
  spine-rs-cli render \
    --json hero.json \
    --atlas hero.atlas \
    --out hero_custom.png \
    --base-skin "Hero_Base" \
    --skins "Hero_Armor,Hero_Shield"
  ```

## License

MIT License. See [LICENSE](LICENSE) for details.

---

Happy rendering! Feel free to open issues or contribute.

