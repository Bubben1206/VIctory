//! Opt-in GPU progress markers for hosts whose DRED history is empty.
//! Counters bound the work in flight; they do not prove a single command
//! caused removal, because later work may start before a marker is visible.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::core::Interface;

pub(super) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_GPU_BREADCRUMBS").is_some())
}

pub(super) struct Breadcrumbs {
    list: ID3D12GraphicsCommandList2,
    buffer: ID3D12Resource,
    mapped: usize,
    gpu: u64,
}

impl Breadcrumbs {
    pub(super) fn new(device: &ID3D12Device, list: &ID3D12GraphicsCommandList) -> Option<Self> {
        let list = list.cast().ok()?;
        let mut buffer: Option<ID3D12Resource> = None;
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: 8,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            ..Default::default()
        };
        let mut mapped = std::ptr::null_mut();
        // SAFETY: plain resource creation and persistent mapping. The owned
        // resource retains its mapping until Drop; writes target COPY_DEST.
        unsafe {
            device
                .CreateCommittedResource(
                    &heap,
                    D3D12_HEAP_FLAG_NONE,
                    &desc,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut buffer,
                )
                .ok()?;
            let buffer = buffer?;
            buffer.Map(0, None, Some(&mut mapped)).ok()?;
            let gpu = buffer.GetGPUVirtualAddress();
            Some(Self {
                list,
                buffer,
                mapped: mapped as usize,
                gpu,
            })
        }
    }

    /// Before submission, after all previous use has finished.
    pub(super) fn clear(&self) {
        // SAFETY: an eight-byte mapping owned by self; the caller holds the
        // host submission lock and has waited for prior GPU use.
        unsafe {
            (self.mapped as *mut u32).write_volatile(0);
            (self.mapped as *mut u32).add(1).write_volatile(0);
        }
    }

    /// Mark the prefix before noted command `at` as completed, and record
    /// that the front end reached this marker. Both indices are zero-based
    /// prefix lengths, so zero also represents no observed GPU progress.
    pub(super) fn before(&self, at: u32) {
        let params = [
            D3D12_WRITEBUFFERIMMEDIATE_PARAMETER {
                Dest: self.gpu + 4,
                Value: at,
            },
            D3D12_WRITEBUFFERIMMEDIATE_PARAMETER {
                Dest: self.gpu,
                Value: at + 1,
            },
        ];
        let modes = [
            D3D12_WRITEBUFFERIMMEDIATE_MODE_MARKER_OUT,
            D3D12_WRITEBUFFERIMMEDIATE_MODE_MARKER_IN,
        ];
        // SAFETY: our open list, live read-back buffer and two matching
        // parameter/mode arrays copied into the command stream.
        unsafe {
            self.list
                .WriteBufferImmediate(2, params.as_ptr(), Some(modes.as_ptr()))
        };
    }

    pub(super) fn end(&self, count: u32) {
        let param = D3D12_WRITEBUFFERIMMEDIATE_PARAMETER {
            Dest: self.gpu + 4,
            Value: count,
        };
        let mode = D3D12_WRITEBUFFERIMMEDIATE_MODE_MARKER_OUT;
        // SAFETY: same ownership and COPY_DEST state as before().
        unsafe { self.list.WriteBufferImmediate(1, &param, Some(&mode)) };
    }

    /// Snapshot after completion or removal. GPU and CPU visibility can
    /// lag: report these as observed marker prefixes, not exact fault sites.
    pub(super) fn progress(&self) -> (u32, u32) {
        // SAFETY: the persistently mapped resource remains owned by self.
        unsafe {
            (
                (self.mapped as *const u32).read_volatile(),
                (self.mapped as *const u32).add(1).read_volatile(),
            )
        }
    }
}

impl Drop for Breadcrumbs {
    fn drop(&mut self) {
        // SAFETY: the mapping belongs to our resource. HostList lifetime
        // rules require its last submission to finish before release.
        unsafe { self.buffer.Unmap(0, None) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "isolated WARP device-removal probe; set VICTORY_D3D12_WARP=1"]
    fn custom_markers_survive_explicit_warp_device_removal() {
        assert_eq!(std::env::var("VICTORY_D3D12_WARP").as_deref(), Ok("1"));
        // Independent device: removing it does not poison the shared host
        // used by other tests. No pending GPU work at explicit removal.
        let host = crate::host::create().unwrap();
        let run = host.new_list().unwrap();
        let markers = Breadcrumbs::new(&host.device, &run.list).unwrap();
        markers.before(0);
        run.notes
            .lock()
            .unwrap()
            .push(("first probe operation".into(), None));
        markers.before(1);
        run.notes
            .lock()
            .unwrap()
            .push(("second probe operation".into(), None));
        run.breadcrumbs.set(Some(markers)).ok().unwrap();
        run.close();
        host.execute(&[&run]);
        let markers = run.breadcrumbs.get().unwrap().as_ref().unwrap();
        assert_eq!(markers.progress(), (2, 2));

        // A shorter recording cannot retain the previous larger prefix.
        run.reset();
        markers.before(0);
        run.notes
            .lock()
            .unwrap()
            .push(("shorter probe operation".into(), None));
        run.close();
        host.execute(&[&run]);
        assert_eq!(markers.progress(), (1, 1));
        // SAFETY: deliberate removal of this isolated software device,
        // after its submissions finished; no physical GPU reset requested.
        unsafe { host.device.cast::<ID3D12Device5>().unwrap().RemoveDevice() };
        // SAFETY: a plain query of our removed device.
        assert!(unsafe { host.device.GetDeviceRemovedReason() }.is_err());
        assert_eq!(markers.progress(), (1, 1));
    }
}
