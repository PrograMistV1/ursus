use crate::PushConstantDesc;
use crate::expr::{BinOp, BuiltinVar, IntrinsicFn};
use crate::shader::{BindingKind, FragmentShader};
use crate::types::Type;
use crate::validate::{TypedExpr, TypedExprKind};
use rspirv::dr::{Builder, Module, Operand};
use rspirv::spirv;
use rspirv::spirv::GlslStd450Op;
use std::collections::HashMap;

struct LoweringContext {
    builder: Builder,
    type_cache: HashMap<Type, spirv::Word>,
    glsl_ext: spirv::Word,
    builtin: HashMap<BuiltinVar, spirv::Word>,
    bindings: HashMap<&'static str, spirv::Word>,
    push_constant_var: Option<(PushConstantDesc, spirv::Word)>,
}

/// Lowers an already-validated, type-annotated shader tree to a SPIR-V
/// `Module`. Callers get a `TypedExpr` from `validate::validate` - this
/// function trusts its type annotations completely and does not re-infer
/// or re-check anything; an internally inconsistent `TypedExpr` (one not
/// actually produced by `validate::validate`) will panic rather than
/// return an error.
pub fn lower(shader: &FragmentShader, typed_output: &TypedExpr) -> Module {
    let mut ctx = LoweringContext::new();

    ctx.declare_builtin(typed_output);
    ctx.declare_bindings(shader);
    ctx.declare_push_constant(shader);

    let output_ty = ctx.spirv_type(typed_output.ty);
    let output_ptr_ty = ctx.builder.type_pointer(None, spirv::StorageClass::Output, output_ty);
    let out_color = ctx.builder.variable(output_ptr_ty, None, spirv::StorageClass::Output, None);
    ctx.builder.decorate(out_color, spirv::Decoration::Location, vec![Operand::LiteralBit32(0)]);

    let type_void = ctx.builder.type_void();
    let type_fn_void = ctx.builder.type_function(type_void, vec![]);

    let main_fn = ctx
        .builder
        .begin_function(type_void, None, spirv::FunctionControl::NONE, type_fn_void)
        .expect("begin_function should not fail for a well-formed void() signature");
    ctx.builder.begin_block(None).expect("begin_block should not fail immediately after begin_function");

    let result = ctx.lower_expr(typed_output);
    ctx.builder.store(out_color, result, None, vec![]).expect("store should not fail: types are pre-validated");

    ctx.builder.ret().expect("ret should not fail at the end of a void function's only block");
    ctx.builder.end_function().expect("end_function should not fail immediately after ret");

    let mut interface = vec![out_color];
    interface.extend(ctx.builtin.values().copied());
    ctx.builder.entry_point(spirv::ExecutionModel::Fragment, main_fn, "main", interface);
    ctx.builder.execution_mode(main_fn, spirv::ExecutionMode::OriginUpperLeft, vec![]);

    let _ = ctx.glsl_ext;

    ctx.builder.module()
}

impl LoweringContext {
    fn new() -> Self {
        let mut builder = Builder::new();
        builder.set_version(1, 0);
        builder.capability(spirv::Capability::Shader);
        let glsl_ext = builder.ext_inst_import("GLSL.std.450");
        builder.memory_model(spirv::AddressingModel::Logical, spirv::MemoryModel::GLSL450);

        Self {
            builder,
            type_cache: HashMap::new(),
            glsl_ext,
            builtin: HashMap::new(),
            bindings: HashMap::new(),
            push_constant_var: None,
        }
    }

    fn spirv_type(&mut self, ty: Type) -> spirv::Word {
        if let Some(&id) = self.type_cache.get(&ty) {
            return id;
        }

        let id = match ty {
            Type::Bool => self.builder.type_bool(),
            Type::Int => self.builder.type_int(32, 1),
            Type::UInt => self.builder.type_int(32, 0),
            Type::Float => self.builder.type_float(32, None),

            Type::IVec2
            | Type::IVec3
            | Type::IVec4
            | Type::UVec2
            | Type::UVec3
            | Type::UVec4
            | Type::Vec2
            | Type::Vec3
            | Type::Vec4 => {
                let component = self.spirv_type(ty.scalar());
                self.builder.type_vector(component, ty.component_count())
            }

            Type::Sampler2D => {
                let float_ty = self.spirv_type(Type::Float);
                let image_ty =
                    self.builder.type_image(float_ty, spirv::Dim::Dim2D, 0, 0, 0, 1, spirv::ImageFormat::Unknown, None);
                self.builder.type_sampled_image(image_ty)
            }

            Type::SamplerCube | Type::Sampler2DShadow => {
                unimplemented!("LoweringContext::spirv_type: {ty:?} not implemented yet")
            }
        };

        self.type_cache.insert(ty, id);
        id
    }

    fn declare_bindings(&mut self, shader: &FragmentShader) {
        for (name, binding) in &shader.bindings {
            let ty = match binding.kind {
                BindingKind::CombinedImageSampler2D => Type::Sampler2D,
            };
            let sampled_image_ty = self.spirv_type(ty);
            let ptr_ty = self.builder.type_pointer(None, spirv::StorageClass::UniformConstant, sampled_image_ty);
            let var = self.builder.variable(ptr_ty, None, spirv::StorageClass::UniformConstant, None);

            self.builder.decorate(var, spirv::Decoration::DescriptorSet, vec![Operand::LiteralBit32(binding.set)]);
            self.builder.decorate(var, spirv::Decoration::Binding, vec![Operand::LiteralBit32(binding.binding)]);

            self.bindings.insert(name, var);
        }
    }

    fn declare_push_constant(&mut self, shader: &FragmentShader) {
        let Some(pc) = &shader.push_constant else { return };
        if pc.fields.is_empty() {
            return;
        }

        let member_types: Vec<spirv::Word> = pc.fields.iter().map(|f| self.spirv_type(f.ty)).collect();
        let struct_ty = self.builder.type_struct(member_types);

        self.builder.decorate(struct_ty, spirv::Decoration::Block, vec![]);
        for (i, field) in pc.fields.iter().enumerate() {
            self.builder.member_decorate(
                struct_ty,
                i as u32,
                spirv::Decoration::Offset,
                vec![Operand::LiteralBit32(field.offset)],
            );
        }

        let ptr_ty = self.builder.type_pointer(None, spirv::StorageClass::PushConstant, struct_ty);
        let var = self.builder.variable(ptr_ty, None, spirv::StorageClass::PushConstant, None);

        self.push_constant_var = Some((pc.clone(), var));
    }

    fn declare_builtin(&mut self, typed_expr: &TypedExpr) {
        match &typed_expr.kind {
            TypedExprKind::Builtin(builtin) => {
                if !self.builtin.contains_key(builtin) {
                    let builtin_ty = self.spirv_type(builtin.ty());
                    let ptr_ty = self.builder.type_pointer(None, spirv::StorageClass::Input, builtin_ty);
                    let var = self.builder.variable(ptr_ty, None, spirv::StorageClass::Input, None);

                    let builtin_kind = match builtin {
                        BuiltinVar::FragCoord => spirv::BuiltIn::FragCoord,
                        BuiltinVar::VertexIndex => spirv::BuiltIn::VertexIndex,
                        BuiltinVar::InstanceIndex => spirv::BuiltIn::InstanceIndex,
                    };
                    self.builder.decorate(var, spirv::Decoration::BuiltIn, vec![Operand::BuiltIn(builtin_kind)]);
                    self.builtin.insert(*builtin, var);
                }
            }
            TypedExprKind::Swizzle { base, .. } => {
                self.declare_builtin(base);
            }
            TypedExprKind::Binary { lhs, rhs, .. } => {
                self.declare_builtin(lhs);
                self.declare_builtin(rhs);
            }
            TypedExprKind::Sample2D { texture, uv } => {
                self.declare_builtin(texture);
                self.declare_builtin(uv);
            }
            TypedExprKind::VecConstruct { components } => {
                components.iter().for_each(|component| self.declare_builtin(component));
            }
            TypedExprKind::Intrinsic { args, .. } => {
                args.iter().for_each(|arg| self.declare_builtin(arg));
            }
            _ => {}
        }
    }

    /// Lowers one type-annotated node to a SPIR-V value id. `node.ty` is
    /// trusted as-is - never recomputed.
    fn lower_expr(&mut self, node: &TypedExpr) -> spirv::Word {
        match &node.kind {
            TypedExprKind::Constant(v) => {
                let float_ty = self.spirv_type(Type::Float);
                self.builder.constant_bit32(float_ty, v.to_bits())
            }

            TypedExprKind::Builtin(b) => self.lower_builtin(*b),

            TypedExprKind::BindingRef(name) => {
                let var =
                    *self.bindings.get(name).unwrap_or_else(|| panic!("lower_expr: BindingRef({name:?}) not declared"));
                let sampled_image_ty = self.spirv_type(node.ty);
                self.builder
                    .load(sampled_image_ty, None, var, None, vec![])
                    .expect("load should not fail: binding variable/type were declared together")
            }

            TypedExprKind::PushConstantField { offset } => self.lower_push_constant_field(*offset, node.ty),

            TypedExprKind::Swizzle { base, pattern } => {
                let base_val = self.lower_expr(base);
                self.lower_swizzle(base.ty, base_val, pattern, node.ty)
            }

            TypedExprKind::Binary { op, lhs, rhs } => {
                let lhs_val = self.lower_expr(lhs);
                let rhs_val = self.lower_expr(rhs);
                self.lower_binary(*op, lhs_val, rhs_val, node.ty)
            }

            TypedExprKind::Sample2D { texture, uv } => {
                let sampled_image = self.lower_expr(texture);
                let uv_val = self.lower_expr(uv);
                let vec4_ty = self.spirv_type(Type::Vec4);
                self.builder
                    .image_sample_implicit_lod(vec4_ty, None, sampled_image, uv_val, None, vec![])
                    .expect("image_sample_implicit_lod should not fail: types are pre-validated")
            }

            TypedExprKind::VecConstruct { components } => self.lower_vec_construct(node.ty, components),

            TypedExprKind::Intrinsic { func, args } => self.lower_intrinsic(*func, args, node.ty),
        }
    }

    fn lower_builtin(&mut self, builtin: BuiltinVar) -> spirv::Word {
        let ty = self.spirv_type(builtin.ty());
        let var = *self.builtin.get(&builtin).unwrap_or_else(|| panic!("lower_builtin: {builtin:?} not declared"));
        self.builder.load(ty, None, var, None, vec![]).expect("load should not fail for a builtin input variable")
    }

    /// Re-derives the push constant member index from its byte offset.
    /// Assumes tightly-packed fields in declaration order (what
    /// `PushConstantDesc::new` currently guarantees) - revisit if alignment
    /// padding is introduced there.
    fn lower_push_constant_field(&mut self, offset: u32, ty: Type) -> spirv::Word {
        let pc_var =
            self.push_constant_var.as_ref().expect("lower_push_constant_field: no push constant block declared");
        let word = pc_var.1;
        let member_index = pc_var
            .0
            .fields
            .iter()
            .position(|f| f.offset == offset)
            .unwrap_or_else(|| panic!("lower_push_constant_field: push constant has no field at offset {offset}; validate should have rejected this PushConstantField"))
            as u32;

        let uint_ty = self.spirv_type(Type::UInt);
        let index_const = self.builder.constant_bit32(uint_ty, member_index);

        let field_ty = self.spirv_type(ty);
        let ptr_ty = self.builder.type_pointer(None, spirv::StorageClass::PushConstant, field_ty);
        let member_ptr = self
            .builder
            .access_chain(ptr_ty, None, word, vec![index_const])
            .expect("access_chain should not fail: member index is derived from a validated offset");

        self.builder
            .load(field_ty, None, member_ptr, None, vec![])
            .expect("load should not fail: pointer/type match by construction")
    }

    fn lower_swizzle(&mut self, base_ty: Type, base_val: spirv::Word, pattern: &str, result_ty: Type) -> spirv::Word {
        let positional = "xyzw";
        let color = "rgba";
        let uses_positional = pattern.chars().all(|c| positional.contains(c));
        let set = if uses_positional { positional } else { color };

        let indices: Vec<u32> = pattern.chars().map(|c| set.find(c).expect("validated pattern") as u32).collect();
        let _ = base_ty;

        let spirv_result_ty = self.spirv_type(result_ty);

        if indices.len() == 1 {
            self.builder
                .composite_extract(spirv_result_ty, None, base_val, vec![indices[0]])
                .expect("composite_extract should not fail: index range is validated")
        } else {
            self.builder
                .vector_shuffle(spirv_result_ty, None, base_val, base_val, indices)
                .expect("vector_shuffle should not fail: indices are validated in range")
        }
    }

    fn lower_binary(&mut self, op: BinOp, lhs_val: spirv::Word, rhs_val: spirv::Word, result_ty: Type) -> spirv::Word {
        let spirv_result_ty = self.spirv_type(result_ty);
        match op {
            BinOp::Add => self.builder.f_add(spirv_result_ty, None, lhs_val, rhs_val),
            BinOp::Sub => self.builder.f_sub(spirv_result_ty, None, lhs_val, rhs_val),
            BinOp::Mul => self.builder.f_mul(spirv_result_ty, None, lhs_val, rhs_val),
            BinOp::Div => self.builder.f_div(spirv_result_ty, None, lhs_val, rhs_val),
        }
        .expect("binary op should not fail: operand types are pre-validated")
    }

    fn lower_vec_construct(&mut self, ty: Type, components: &[TypedExpr]) -> spirv::Word {
        let spirv_result_ty = self.spirv_type(ty);
        let target_count = ty.component_count();

        let component_vals: Vec<spirv::Word> = components.iter().map(|c| self.lower_expr(c)).collect();

        if components.len() == 1 && target_count > 1 {
            let repeated = vec![component_vals[0]; target_count as usize];
            self.builder
                .composite_construct(spirv_result_ty, None, repeated)
                .expect("composite_construct should not fail: splat form is pre-validated")
        } else {
            self.builder
                .composite_construct(spirv_result_ty, None, component_vals)
                .expect("composite_construct should not fail: component count is pre-validated")
        }
    }

    fn lower_intrinsic(&mut self, func: IntrinsicFn, args: &[TypedExpr], result_ty: Type) -> spirv::Word {
        let arg_vals: Vec<spirv::Word> = args.iter().map(|a| self.lower_expr(a)).collect();
        let spirv_result_ty = self.spirv_type(result_ty);
        let operands = arg_vals.iter().map(|val| Operand::IdRef(*val));

        match func {
            IntrinsicFn::Clamp => {
                let instruction = match args[0].ty.scalar() {
                    Type::Int => GlslStd450Op::SClamp,
                    Type::UInt => GlslStd450Op::UClamp,
                    Type::Float => GlslStd450Op::FClamp,
                    _ => panic!(
                        "lower_intrinsic: Clamp got scalar type {:?}, validate should allow only float, int or uint",
                        args[0].ty
                    ),
                } as u32;
                self.builder
                    .ext_inst(spirv_result_ty, None, self.glsl_ext, instruction, operands)
                    .expect("clamp should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Pow => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Pow as u32, operands)
                .expect("pow should not fail: operand types are pre-validated"),
            IntrinsicFn::Min => {
                let instruction = match args[0].ty.scalar() {
                    Type::Int => GlslStd450Op::SMin,
                    Type::UInt => GlslStd450Op::UMin,
                    Type::Float => GlslStd450Op::FMin,
                    _ => panic!(
                        "lower_intrinsic: Min got scalar type {:?}, validate should allow only float, int or uint",
                        args[0].ty
                    ),
                } as u32;
                self.builder
                    .ext_inst(spirv_result_ty, None, self.glsl_ext, instruction, operands)
                    .expect("min should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Max => {
                let instruction = match args[0].ty.scalar() {
                    Type::Int => GlslStd450Op::SMax,
                    Type::UInt => GlslStd450Op::UMax,
                    Type::Float => GlslStd450Op::FMax,
                    _ => panic!(
                        "lower_intrinsic: Max got scalar type {:?}, validate should allow only float, int or uint",
                        args[0].ty
                    ),
                } as u32;
                self.builder
                    .ext_inst(spirv_result_ty, None, self.glsl_ext, instruction, operands)
                    .expect("max should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Mix => {
                let instruction = match args[0].ty.scalar() {
                    // TODO: IMix requires bool selector; add when int mix is needed (see validate for Mix)
                    Type::Float => GlslStd450Op::FMix,
                    _ => panic!(
                        "lower_intrinsic: Mix got scalar type {:?}, validate should allow only float",
                        args[0].ty
                    ),
                } as u32;
                self.builder
                    .ext_inst(spirv_result_ty, None, self.glsl_ext, instruction, operands)
                    .expect("mix should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Normalize => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Normalize as u32, operands)
                .expect("normalize should not fail: operand types are pre-validated"),
            IntrinsicFn::Dot => {
                // OpDot is a core SPIR-V instruction, not a GLSL.std.450 ext instruction.
                self.builder
                    .dot(spirv_result_ty, None, arg_vals[0], arg_vals[1])
                    .expect("dot should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Cross => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Cross as u32, operands)
                .expect("cross should not fail: operand types are pre-validated"),
            IntrinsicFn::Length => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Length as u32, operands)
                .expect("length should not fail: operand types are pre-validated"),
            IntrinsicFn::Sqrt => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Sqrt as u32, operands)
                .expect("sqrt should not fail: operand types are pre-validated"),
            IntrinsicFn::Abs => {
                let instruction = match args[0].ty.scalar() {
                    Type::Int => GlslStd450Op::SAbs,
                    Type::Float => GlslStd450Op::FAbs,
                    _ => panic!(
                        "lower_intrinsic: Abs got scalar type {:?}, validate should allow only float or int",
                        args[0].ty
                    ),
                } as u32;
                self.builder
                    .ext_inst(spirv_result_ty, None, self.glsl_ext, instruction, operands)
                    .expect("abs should not fail: operand types are pre-validated")
            }
            IntrinsicFn::Floor => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Floor as u32, operands)
                .expect("floor should not fail: operand types are pre-validated"),
            IntrinsicFn::Fract => self
                .builder
                .ext_inst(spirv_result_ty, None, self.glsl_ext, GlslStd450Op::Fract as u32, operands)
                .expect("fract should not fail: operand types are pre-validated"),
        }
    }
}
