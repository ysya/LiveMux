# LiveMux

Create Motion Photo v2/v3 from HEIC or JPG files paired with videos. Output files are compatible with Google Photos and Samsung Gallery as motion/live photos.

This is a **Rust rewrite** of [PetrVys/MotionPhoto2](https://github.com/PetrVys/MotionPhoto2), providing both a CLI tool (`livemux`) and a desktop GUI app built with Tauri.

Key improvements over the original Python version:
- Native performance with Rust
- Cross-platform desktop GUI (Tauri v2)
- Automatic MOV→MP4 remux for iPhone Live Photos (via FFmpeg)
- Apple Live Photo metadata cleanup for better Google Photos compatibility

For iPhone Live Photos, the presentation timestamp is migrated so the photo starts from the same keyframe.

## Requirements

- [ExifTool](https://exiftool.org/) — required
- [FFmpeg](https://ffmpeg.org/) — optional, needed to remux iPhone MOV videos to MP4 for Google Photos compatibility

## Installation

### From Release

Download the release for your platform from the [Releases](https://github.com/ysya/LiveMux/releases) page.

### Building from Source

```bash
# CLI only
cargo build --release -p livemux-cli

# GUI app (requires Node.js + pnpm)
cd ui && pnpm install && cd ..
cargo tauri build
```

## Usage

### GUI

Run the app — it provides a graphical interface with two modes:
- **Single Mux**: Select an image and video pair to create a motion photo
- **Batch Directory**: Process an entire directory of image-video pairs

### CLI

#### Single file

```bash
livemux mux --image IMG_1234.HEIC --video IMG_1234.MOV
```

#### Directory mode

```bash
livemux dir --directory /your/directory
```

With EXIF metadata matching (useful for Google Takeout or iCloud exports where filenames may not match):

```bash
livemux dir --directory /your/directory --exif-match
```

### Options

| Flag | Description |
|---|---|
| `--overwrite` | Replace the original image file |
| `--delete-video` | Remove source video after muxing |
| `--keep-temp` | Keep intermediate XMP sidecar files |
| `--no-xmp` | Skip XMP metadata embedding (faster, less compatible) |
| `--recursive` | Process subdirectories recursively |
| `--exif-match` | Match image-video pairs by EXIF metadata |
| `--incremental` | Skip files that already contain motion photo data |
| `--copy-unmuxed` | Copy unmatched images to output directory |
| `-v, --verbose` | Enable debug logging |

## Limitations

HDR in Google Photos works only for HEIC photos with HDR stored in ISO/CD 21496-1 format. Effectively, HEIC photos must be shot on iPhone 15+ with iOS 18+ to be recognized as HDR by Google Photos.

## Credits

This project is a Rust rewrite based on the original [MotionPhoto2](https://github.com/PetrVys/MotionPhoto2) by [Petr Vyskocil](https://github.com/PetrVys), licensed under MIT.

Thanks to the original contributors: [@Tkd-Alex](https://github.com/Tkd-Alex), [@NightMean](https://github.com/NightMean), [@sahilph](https://github.com/sahilph), [@tribut](https://github.com/tribut), [@4Urban](https://github.com/4Urban), [@IamRysing](https://github.com/IamRysing).

### References

- [Google Motion Photo Format](https://developer.android.com/media/platform/motion-photo-format)
- [Samsung trailer tags documentation](https://github.com/doodspav/motionphoto)

## License

[MIT](LICENSE)
