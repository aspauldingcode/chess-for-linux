fn main() {
    println!("cargo:rerun-if-changed=../src/board.wgsl");
    let src = host_wgsl(&std::fs::read_to_string("../src/board.wgsl").expect("board.wgsl"));
    let module = naga::front::wgsl::parse_str(&src).expect("parse board.wgsl");
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let info = validator.validate(&module).expect("validate board.wgsl");
    let words =
        naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
            .expect("spirv");
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("board.spv");
    std::fs::write(out, bytes).unwrap();
}

/// iOS simulator MoltenVK returns `VK_ERROR_FEATURE_NOT_PRESENT` when the
/// fragment shader indexes a `texture_2d_array` (`shaderSampledImageArrayDynamicIndexing`).
/// The native `chess-linux` shader stays an array. This host copy packs the
/// same layers into an 8x4 grid of 256px cells.
fn host_wgsl(src: &str) -> String {
    let src = src.replace(
        "@group(0) @binding(1) var surfaces: texture_2d_array<f32>;",
        "@group(0) @binding(1) var surfaces: texture_2d<f32>;",
    );
    let src = src.replace(
        "if v.params.x>=0.0 {base*=textureSample(surfaces,surface_sampler,v.uv,i32(v.params.x));}",
        "if v.params.x>=0.0 {base*=textureSample(surfaces,surface_sampler,atlas_uv(v.uv,v.params.x));}",
    );
    let helper = r"
const ATLAS_COLS: f32 = 8.0;
const ATLAS_ROWS: f32 = 4.0;
fn atlas_uv(uv: vec2<f32>, layer: f32) -> vec2<f32> {
  let col = layer % ATLAS_COLS;
  let row = floor(layer / ATLAS_COLS);
  return vec2<f32>((uv.x + col) / ATLAS_COLS, (uv.y + row) / ATLAS_ROWS);
}
";
    src.replacen("@fragment", &format!("{helper}@fragment"), 1)
}
