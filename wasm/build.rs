fn main() {
    println!("cargo:rerun-if-changed=../src/board.wgsl");
    let src = std::fs::read_to_string("../src/board.wgsl").expect("board.wgsl");
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
