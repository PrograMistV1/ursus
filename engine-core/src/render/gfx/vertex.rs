use crate::render::gfx::Format;
use ash::vk;

#[derive(Debug, Clone, Copy)]
pub struct VertexAttribute {
    pub location: u32,
    pub format: Format,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct VertexLayout {
    pub stride: u32,
    pub attributes: Vec<VertexAttribute>,
}

impl VertexLayout {
    pub fn only_locations(&self, locations: &[u32]) -> VertexLayout {
        let attributes: Vec<VertexAttribute> =
            self.attributes.iter().filter(|a| locations.contains(&a.location)).copied().collect();

        assert_eq!(
            attributes.len(),
            locations.len(),
            "VertexLayout::only_locations: requested location {:?}, but only {:?} available",
            locations,
            self.attributes.iter().map(|a| a.location).collect::<Vec<_>>()
        );

        VertexLayout { stride: self.stride, attributes }
    }

    pub(crate) fn to_vk_binding(&self, binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(binding)
            .stride(self.stride)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub(crate) fn to_vk_attributes(&self, binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        self.attributes
            .iter()
            .map(|a| {
                vk::VertexInputAttributeDescription::default()
                    .binding(binding)
                    .location(a.location)
                    .format(a.format.to_vk())
                    .offset(a.offset)
            })
            .collect()
    }
}

pub trait VertexFormat: bytemuck::Pod {
    fn layout() -> VertexLayout;
}
