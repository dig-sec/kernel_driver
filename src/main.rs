#![no_std]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::boxed::Box;
use ntapi::{ntkeapi::*, ntoskrnl::*, ntmmapi::*, ntioapi::*};
use wdk_sys::*;

struct KernelDriver {
    original_nt_create_file: Option<unsafe extern "system" fn(
        *mut HANDLE,
        ACCESS_MASK,
        *mut OBJECT_ATTRIBUTES,
        *mut IO_STATUS_BLOCK,
        *mut LARGE_INTEGER,
        ULONG,
        ULONG,
        ULONG,
        ULONG,
        PVOID,
        ULONG,
    ) -> NTSTATUS>,
    driver_object: *mut DRIVER_OBJECT,
}

impl KernelDriver {
    fn new(driver: *mut DRIVER_OBJECT) -> Self {
        Self {
            original_nt_create_file: None,
            driver_object: driver,
        }
    }

    fn hook_system_calls(&mut self) {
        unsafe {
            let nt_create_file_addr = MmGetSystemRoutineAddress(&UNICODE_STRING {
                Length: 24,
                MaximumLength: 26,
                Buffer: b"NtCreateFile\0".as_ptr() as *mut u16,
            });

            if !nt_create_file_addr.is_null() {
                self.original_nt_create_file = Some(core::mem::transmute(nt_create_file_addr));

                let original = self.original_nt_create_file;
                let hooked_nt_create_file = Box::new(
                    move |handle: *mut HANDLE,
                          access_mask: ACCESS_MASK,
                          object_attributes: *mut OBJECT_ATTRIBUTES,
                          io_status_block: *mut IO_STATUS_BLOCK,
                          allocation_size: *mut LARGE_INTEGER,
                          file_attributes: ULONG,
                          share_access: ULONG,
                          create_disposition: ULONG,
                          create_options: ULONG,
                          ea_buffer: PVOID,
                          ea_length: ULONG|
                          -> NTSTATUS {
                        DbgPrint("NtCreateFile called\0".as_ptr() as *const i8);
                        
                        if let Some(original) = original {
                            original(
                                handle,
                                access_mask,
                                object_attributes,
                                io_status_block,
                                allocation_size,
                                file_attributes,
                                share_access,
                                create_disposition,
                                create_options,
                                ea_buffer,
                                ea_length,
                            )
                        } else {
                            STATUS_UNSUCCESSFUL
                        }
                    },
                );

                let hooked_fn_ptr: unsafe extern "system" fn(
                    *mut HANDLE,
                    ACCESS_MASK,
                    *mut OBJECT_ATTRIBUTES,
                    *mut IO_STATUS_BLOCK,
                    *mut LARGE_INTEGER,
                    ULONG,
                    ULONG,
                    ULONG,
                    ULONG,
                    PVOID,
                    ULONG,
                ) -> NTSTATUS = Box::into_raw(hooked_nt_create_file) as _;

                core::ptr::write_volatile(nt_create_file_addr as *mut _, hooked_fn_ptr);
            }
        }
    }

    fn hook_file_operations(&self) {
        unsafe {
            (*self.driver_object).MajorFunction[IRP_MJ_READ as usize] = Some(hooked_read);
            (*self.driver_object).MajorFunction[IRP_MJ_WRITE as usize] = Some(hooked_write);
        }
    }

    fn hook_network_activity(&self) {
        unsafe {
            (*self.driver_object).MajorFunction[IRP_MJ_CREATE as usize] = Some(hooked_create);
        }
    }
}

unsafe extern "system" fn hooked_read(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    DbgPrint("File read operation detected\0".as_ptr() as *const i8);
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

unsafe extern "system" fn hooked_write(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    DbgPrint("File write operation detected\0".as_ptr() as *const i8);
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

unsafe extern "system" fn hooked_create(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    DbgPrint("Network connection attempt detected\0".as_ptr() as *const i8);
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

#[no_mangle]
pub extern "system" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    _registry_path: *mut UNICODE_STRING,
) -> NTSTATUS {
    let mut kernel_driver = KernelDriver::new(driver);
    kernel_driver.hook_system_calls();
    kernel_driver.hook_file_operations();
    kernel_driver.hook_network_activity();
    STATUS_SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[alloc_error_handler]
fn alloc_error(_layout: core::alloc::Layout) -> ! {
    loop {}
}
