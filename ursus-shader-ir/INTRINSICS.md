# Intrinsic Functions: Allowed Operand Types

Notation:

- `float` means `Float` (scalar) or `Vec2`, `Vec3`, `Vec4` (float vectors).
- `int` means `Int` (scalar) or `IVec2`, `IVec3`, `IVec4` (signed int vectors).
- `uint` means `UInt` (scalar) or `UVec2`, `UVec3`, `UVec4` (unsigned int vectors).
- `Bool` and opaque types (`Sampler2D`, `SamplerCube`, `Sampler2DShadow`) are never allowed as operands of any
  intrinsic.
- "Same type" means all operands have exactly the same type, including vector size. Scalars are never implicitly
  broadcast to vectors.

| Function           | Operands                                                               | Result           | Notes                                                                                                               |
|--------------------|------------------------------------------------------------------------|------------------|---------------------------------------------------------------------------------------------------------------------|
| `Clamp(x, lo, hi)` | `x`, `lo`, `hi`: float, or int, or uint. All three have the same type. | Same type as `x` | Scalar bounds for a vector `x` are not allowed. Bounds must be built as vectors.                                    |
| `Pow(x, y)`        | `x`, `y`: float, same type                                             | Same type as `x` | Float only.                                                                                                         |
| `Min(x, y)`        | `x`, `y`: float, or int, or uint, same type                            | Same type as `x` | Scalar second operand is not allowed.                                                                               |
| `Max(x, y)`        | `x`, `y`: float, or int, or uint, same type                            | Same type as `x` | Scalar second operand is not allowed.                                                                               |
| `Mix(x, y, a)`     | `x`, `y`, `a`: float, same type                                        | Same type as `x` | Float only. `a` must match `x` exactly, including vector size. Int `Mix` needs a bool selector and is out of scope. |
| `Normalize(x)`     | `x`: float scalar or float vector                                      | Same type as `x` |                                                                                                                     |
| `Dot(x, y)`        | `x`, `y`: float vector, same type                                      | `Float`          | Core SPIR-V instruction `OpDot`, not a `GLSL.std.450` instruction. Scalars are rejected.                            |
| `Cross(x, y)`      | `x`, `y`: `Vec3` only                                                  | `Vec3`           |                                                                                                                     |
| `Length(x)`        | `x`: float scalar or float vector                                      | `Float`          | Result is always the scalar component type, even for vector input.                                                  |
| `Sqrt(x)`          | `x`: float scalar or float vector                                      | Same type as `x` |                                                                                                                     |
| `Abs(x)`           | `x`: float or int, scalar or vector                                    | Same type as `x` | Instruction depends on the scalar component: `FAbs` for float, `SAbs` for int.                                      |
| `Floor(x)`         | `x`: float scalar or float vector                                      | Same type as `x` |                                                                                                                     |
| `Fract(x)`         | `x`: float scalar or float vector                                      | Same type as `x` |                                                                                                                     |