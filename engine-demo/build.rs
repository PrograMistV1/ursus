use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let target_dir = Path::new(&out_dir).ancestors().nth(3).expect("не удалось вычислить target dir из OUT_DIR");

    let src = Path::new(&manifest_dir).join("assets");
    let dst = target_dir.join("assets");

    println!("cargo:rerun-if-changed={}", src.display());

    if src.exists() {
        copy_dir_recursive(&src, &dst).expect("не удалось скопировать assets");
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
