// SPDX-License-Identifier: MIT OR Apache-2.0

//! The three strings of the GPU adapter the F3 screen prints in place of the
//! OpenGL vendor, renderer and version lines of the reference
//! (`GuiOverlayDebug.getDebugInfoRight`).

/// Name, driver and backend of the adapter the renderer runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterSummary {
    /// The adapter name (`GL_RENDERER` in the reference).
    pub name: String,
    /// The driver name, or the device class when the driver reports none
    /// (`GL_VENDOR` in the reference).
    pub vendor: String,
    /// The graphics API in use, such as `Vulkan` (`GL_VERSION`).
    pub backend: String,
}

impl AdapterSummary {
    /// Summarises the adapter the device was created from.
    pub(crate) fn of(adapter: &wgpu::Adapter) -> Self {
        Self::from_info(&adapter.get_info())
    }

    /// Summarises the adapter description `info`.
    pub(crate) fn from_info(info: &wgpu::AdapterInfo) -> Self {
        let vendor = if info.driver.is_empty() {
            format!("{:?}", info.device_type)
        } else {
            info.driver.clone()
        };
        let backend = if info.driver_info.is_empty() {
            format!("{:?}", info.backend)
        } else {
            format!("{:?} {}", info.backend, info.driver_info)
        };
        Self {
            name: info.name.clone(),
            vendor,
            backend,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(driver: &str, driver_info: &str) -> wgpu::AdapterInfo {
        let mut info = wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        info.name = "Test GPU".to_owned();
        info.driver = driver.to_owned();
        info.driver_info = driver_info.to_owned();
        info
    }

    #[test]
    fn the_driver_names_the_vendor_when_it_is_known() {
        let summary = AdapterSummary::from_info(&info("radv", "Mesa 25"));
        assert_eq!(summary.name, "Test GPU");
        assert_eq!(summary.vendor, "radv");
        assert_eq!(summary.backend, "Vulkan Mesa 25");
    }

    #[test]
    fn the_device_class_replaces_a_missing_driver() {
        let summary = AdapterSummary::from_info(&info("", ""));
        assert_eq!(summary.vendor, "DiscreteGpu");
        assert_eq!(summary.backend, "Vulkan");
    }
}
