pub mod font_atlas;
pub mod font_manager;
pub mod gpu_font_manager;

pub use font_atlas::FontAtlas;

pub const DEFAULT_CHARSET: &str = concat!(
    " !\"#$%&'()*+,-./0123456789:;<=>?@",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`",
    "abcdefghijklmnopqrstuvwxyz{|}~",
    "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя",
);

pub const DEFAULT_FONT_SIZES: &[u32] = &[12, 14, 18, 24, 32];
