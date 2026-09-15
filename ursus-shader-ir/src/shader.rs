use crate::expr::Expr;
use crate::types::Type;

/// A single resource binding, e.g.
/// `layout (set = 0, binding = 0) uniform sampler2D hdrInput`.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub set: u32,
    pub binding: u32,
    pub kind: BindingKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    CombinedImageSampler2D,
}

/// One field of the shader's push constant block, in declaration order.
#[derive(Debug, Clone, Copy)]
pub struct PushConstantFieldDesc {
    pub name: &'static str,
    pub offset: u32,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct PushConstantDesc {
    pub fields: Vec<PushConstantFieldDesc>,
}

impl PushConstantDesc {
    /// Builds a push constant block description from `(name, ty)` pairs,
    /// computing offsets in declaration order. No std430/std140 alignment
    /// padding applied yet - only tightly-packed scalar/vector fields are
    /// supported.
    pub fn new(fields: &[(&'static str, Type)]) -> Self {
        let mut offset = 0u32;
        let fields = fields
            .iter()
            .map(|&(name, ty)| {
                let field = PushConstantFieldDesc { name, offset, ty };
                offset += ty.component_byte_size() * ty.component_count();
                field
            })
            .collect();
        Self { fields }
    }

    /// Builds an `Expr::PushConstantField` node referencing the field named
    /// `name`. Panics if no field with that name was declared.
    pub fn field(&self, name: &str) -> Expr {
        let field = self
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("PushConstantDesc::field: no field named '{name}' in push constant block"));
        Expr::PushConstantField { offset: field.offset, ty: field.ty }
    }
}

/// A full fragment shader: its resource bindings, optional push constant
/// block, and the expression producing its single color output.
///
/// Only one color output (location 0) is supported - multi-render-target
/// output is not implemented yet.
#[derive(Debug, Clone)]
pub struct FragmentShader {
    pub bindings: Vec<(&'static str, Binding)>,
    pub push_constant: Option<PushConstantDesc>,
    pub output: Expr,
}

impl FragmentShader {
    pub fn binding(&self, name: &str) -> &Binding {
        self.bindings
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| b)
            .unwrap_or_else(|| panic!("FragmentShader::binding: no binding named '{name}'"))
    }
}
