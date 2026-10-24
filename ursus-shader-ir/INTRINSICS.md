# Intrinsic Functions: Allowed Operand Types

Notation:

- `float` means `Float` (scalar) or `Vec2`, `Vec3`, `Vec4` (float vectors).
- `int` means `Int` (scalar) or `IVec2`, `IVec3`, `IVec4` (signed int vectors).
- `uint` means `UInt` (scalar) or `UVec2`, `UVec3`, `UVec4` (unsigned int vectors).
- `Bool` and opaque types (`Sampler2D`, `SamplerCube`, `Sampler2DShadow`) are never allowed as operands of any
  intrinsic.
- "Same type" means all operands have exactly the same type, including vector size. Scalars are never implicitly
  broadcast to vectors.

| Function           | Operands                                           | Result           |
|--------------------|----------------------------------------------------|------------------|
| `Clamp(x, lo, hi)` | `x`, `lo`, `hi`: float, or int, or uint, same type | Same type as `x` |
| `Pow(x, y)`        | `x`, `y`: float, same type                         | Same type as `x` |
| `Min(x, y)`        | `x`, `y`: float, or int, or uint, same type        | Same type as `x` |
| `Max(x, y)`        | `x`, `y`: float, or int, or uint, same type        | Same type as `x` |
| `Mix(x, y, a)`     | `x`, `y`, `a`: `float` only                        | Same type as `x` |
| `Normalize(x)`     | `x`: float                                         | Same type as `x` |
| `Dot(x, y)`        | `x`, `y`: float vector, same type                  | `Float`          |
| `Cross(x, y)`      | `x`, `y`: `Vec3` only                              | Same type as `x` |
| `Length(x)`        | `x`: float                                         | `Float` (scalar) |
| `Sqrt(x)`          | `x`: float                                         | Same type as `x` |
| `Abs(x)`           | `x`: float or int                                  | Same type as `x` |
| `Floor(x)`         | `x`: float                                         | Same type as `x` |
| `Fract(x)`         | `x`: float                                         | Same type as `x` |
