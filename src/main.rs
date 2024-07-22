#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use ntapi::{ntkeapi::*, ntoskrnl::*, ntmmapi::*, ntioapi::*};
use wdk_sys::*;

const MAX_LOGS: usize = 1000;
static LOG_COUNT: AtomicUsize = AtomicUsize::new(0);
static mut LOGS: [u8; MAX_LOGS * 100] = [0; MAX_LOGS * 100];

const FILE_DEVICE_UNKNOWN: u32 = 0x00000022;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

const IOCTL_GET_LOGS: u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS);
const IOCTL_CLEAR_LOGS: u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS);

const fn CTL_CODE(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

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
    original_nt_open_file: Option<unsafe extern "system" fn(
        *mut HANDLE,
        ACCESS_MASK,
        *mut OBJECT_ATTRIBUTES,
        *mut IO_STATUS_BLOCK,
        ULONG,
        ULONG,
    ) -> NTSTATUS>,
    driver_object: *mut DRIVER_OBJECT,
    device_object: *mut DEVICE_OBJECT,
}

impl KernelDriver {
    fn new(driver: *mut DRIVER_OBJECT) -> Self {
        Self {
            original_nt_create_file: None,
            original_nt_open_file: None,
            driver_object: driver,
            device_object: core::ptr::null_mut(),
        }
    }

    fn hook_system_calls(&mut self) {
        unsafe {
            self.hook_nt_create_file();
            self.hook_nt_open_file();
        }
    }

    unsafe fn hook_nt_create_file(&mut self) {
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
                    log_event("NtCreateFile called");
                    
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

    unsafe fn hook_nt_open_file(&mut self) {
        let nt_open_file_addr = MmGetSystemRoutineAddress(&UNICODE_STRING {
            Length: 22,
            MaximumLength: 24,
            Buffer: b"NtOpenFile\0".as_ptr() as *mut u16,
        });

        if !nt_open_file_addr.is_null() {
            self.original_nt_open_file = Some(core::mem::transmute(nt_open_file_addr));

            let original = self.original_nt_open_file;
            let hooked_nt_open_file = Box::new(
                move |handle: *mut HANDLE,
                      access_mask: ACCESS_MASK,
                      object_attributes: *mut OBJECT_ATTRIBUTES,
                      io_status_block: *mut IO_STATUS_BLOCK,
                      share_access: ULONG,
                      open_options: ULONG|
                      -> NTSTATUS {
                    log_event("NtOpenFile called");
                    
                    if let Some(original) = original {
                        original(
                            handle,
                            access_mask,
                            object_attributes,
                            io_status_block,
                            share_access,
                            open_options,
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
                ULONG,
                ULONG,
            ) -> NTSTATUS = Box::into_raw(hooked_nt_open_file) as _;

            core::ptr::write_volatile(nt_open_file_addr as *mut _, hooked_fn_ptr);
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

    fn create_device(&mut self) -> NTSTATUS {
        unsafe {
            let device_name = UNICODE_STRING::from("\\Device\\KernelDriverMonitor");
            let mut device_object: *mut DEVICE_OBJECT = core::ptr::null_mut();
            let status = IoCreateDevice(
                self.driver_object,
                0,
                &device_name,
                FILE_DEVICE_UNKNOWN,
                0,
                FALSE as BOOLEAN,
                &mut device_object,
            );

            if NT_SUCCESS(status) {
                self.device_object = device_object;
                (*self.driver_object).MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(device_control);
                
                // Create symbolic link
                let dos_device_name = UNICODE_STRING::from("\\DosDevices\\KernelDriverMonitor");
                IoCreateSymbolicLink(&dos_device_name, &device_name);
            }

            status
        }
    }
}

fn log_event(message: &str) {
    let count = LOG_COUNT.fetch_add(1, Ordering::SeqCst);
    if count < MAX_LOGS {
        let message_bytes = message.as_bytes();
        let max_len = core::cmp::min(message_bytes.len(), 99);
        unsafe {
            let start = count * 100;
            LOGS[start] = max_len as u8;
            LOGS[start + 1..start + 1 + max_len].copy_from_slice(&message_bytes[..max_len]);
        }
    }
}

unsafe extern "system" fn hooked_read(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    log_event("File read operation detected");
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

unsafe extern "system" fn hooked_write(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    log_event("File write operation detected");
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

unsafe extern "system" fn hooked_create(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    log_event("Network connection attempt detected");
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    STATUS_SUCCESS
}

unsafe extern "system" fn device_control(
    device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    let stack_location = IoGetCurrentIrpStackLocation(irp);
    let io_control_code = (*stack_location).Parameters.DeviceIoControl().IoControlCode;

    match io_control_code {
        IOCTL_GET_LOGS => {
            let buffer = (*irp).AssociatedIrp.SystemBuffer as *mut u8;
            let buffer_length = (*stack_location).Parameters.DeviceIoControl().OutputBufferLength as usize;
            let log_count = LOG_COUNT.load(Ordering::SeqCst);
            let data_length = core::cmp::min(log_count * 100, buffer_length);

            core::ptr::copy_nonoverlapping(LOGS.as_ptr(), buffer, data_length);

            (*irp).IoStatus().Information = data_length as u64;
            (*irp).IoStatus().Status = STATUS_SUCCESS;
        },
        IOCTL_CLEAR_LOGS => {
            LOG_COUNT.store(0, Ordering::SeqCst);
            (*irp).IoStatus().Information = 0;
            (*irp).IoStatus().Status = STATUS_SUCCESS;
        },
        _ => {
            (*irp).IoStatus().Status = STATUS_INVALID_DEVICE_REQUEST;
        }
    }

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
    
    let status = kernel_driver.create_device();
    if !NT_SUCCESS(status) {
        return status;
    }

    unsafe {
        (*driver).DriverUnload = Some(driver_unload);
    }

    STATUS_SUCCESS
}

unsafe extern "system" fn driver_unload(driver_object: *mut DRIVER_OBJECT) {
    let dos_device_name = UNICODE_STRING::from("\\DosDevices\\KernelDriverMonitor");
    IoDeleteSymbolicLink(&dos_device_name);

    if let Some(device_object) = (*driver_object).DeviceObject.as_mut() {
        IoDeleteDevice(device_object);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[alloc_error_handler]
fn alloc_error(_layout: core::alloc::Layout) -> ! {
    loop {}
}