use crate::{BinOp, Expr, IntrinsicFn};

// Swizzle accessors for all 1-4 component combinations of xyzw and rgba.
include!(concat!(env!("OUT_DIR"), "/swizzles.rs"));

pub fn add(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Add, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}

pub fn sub(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Sub, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}

pub fn mul(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Mul, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}

pub fn div(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Div, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}

pub fn swizzle(base: Expr, pattern: &'static str) -> Expr {
    Expr::Swizzle { base: Box::new(base), pattern }
}

pub fn clamp(value: Expr, min: Expr, max: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Clamp, args: vec![value, min, max] }
}

pub fn pow(base: Expr, exponent: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Pow, args: vec![base, exponent] }
}

pub fn min(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Min, args: vec![lhs, rhs] }
}

pub fn max(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Max, args: vec![lhs, rhs] }
}

pub fn mix(x: Expr, y: Expr, a: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Mix, args: vec![x, y, a] }
}

pub fn normalize(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Normalize, args: vec![v] }
}

pub fn dot(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Dot, args: vec![lhs, rhs] }
}

pub fn cross(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Cross, args: vec![lhs, rhs] }
}

pub fn length(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Length, args: vec![v] }
}

pub fn sqrt(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Sqrt, args: vec![v] }
}

pub fn abs(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Abs, args: vec![v] }
}

pub fn floor(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Floor, args: vec![v] }
}

pub fn fract(v: Expr) -> Expr {
    Expr::Intrinsic { func: IntrinsicFn::Fract, args: vec![v] }
}

#[macro_export]
macro_rules! vec2 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::Vec2, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! vec3 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::Vec3, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! vec4 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::Vec4, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! ivec2 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::IVec2, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! ivec3 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::IVec3, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! ivec4 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::IVec4, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! uvec2 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::UVec2, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! uvec3 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::UVec3, components: vec![$($x),+] }
    };
}

#[macro_export]
macro_rules! uvec4 {
    ($($x:expr),+) => {
        $crate::Expr::VecConstruct { ty: $crate::Type::UVec4, components: vec![$($x),+] }
    };
}

pub fn sampler2d(texture: Expr, uv: Expr) -> Expr {
    Expr::Sample2D { texture: Box::new(texture), uv: Box::new(uv) }
}
