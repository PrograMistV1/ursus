use crate::FragmentShader;
use crate::lower::lower;
use crate::validate::{ValidationError, validate};
use rspirv::binary::Assemble;

pub fn compile_fragment(shader: &FragmentShader) -> Result<Vec<u8>, ValidationError> {
    let typed = validate(shader)?;
    let words = lower(shader, &typed).assemble();
    Ok(words.iter().flat_map(|w| w.to_le_bytes()).collect())
}
