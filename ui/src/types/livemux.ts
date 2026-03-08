export interface MuxConfig {
  image_path: string
  video_path: string
  output_path: string | null
  output_directory: string | null
  delete_video: boolean
  delete_temp: boolean
  overwrite: boolean
  no_xmp: boolean
}

export interface BatchConfig {
  directory: string
  output_dir: string | null
  recursive: boolean
  exif_match: boolean
  incremental: boolean
  copy_unmuxed: boolean
  delete_video: boolean
  delete_temp: boolean
  overwrite: boolean
}

export interface BatchProgress {
  current: number
  total: number
  file: string
  status: "processing" | "done" | "error" | "skipped"
  success: boolean
  error: string | null
}
