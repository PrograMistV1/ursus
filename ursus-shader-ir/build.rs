use std::{env, fs, path::PathBuf};

fn main() {
    let mut s = String::from("impl Expr {\n");
    for set in ["xyzw", "rgba"] {
        let chars: Vec<char> = set.chars().collect();
        for len in 1..=4u32 {
            for n in 0..4usize.pow(len) {
                let name: String = (0..len).map(|i| chars[(n / 4usize.pow(i)) % 4]).collect();
                s.push_str(&format!("    pub fn {name}(self) -> Expr {{ swizzle(self, \"{name}\") }}\n"));
            }
        }
    }
    s.push_str("}\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("swizzles.rs");
    fs::write(out, s).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
