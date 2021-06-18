// libiio-sys/src/buffer.rs
//
// Copyright (c) 2018-2021, Frank Pagliughi
//
// Licensed under the MIT license:
//   <LICENSE or http://opensource.org/licenses/MIT>
// This file may not be copied, modified, or distributed except according
// to those terms.
//
//! Industrial I/O Buffers.
//!
//! The process of capturing samples from or uploading samples to hardware is
//! managed using [`Buffer`] and related methods.
//!
//! It is important to keep in mind that an instance of [`Buffer`] is always
//! coupled to exactly **one instance of [`Device`]**, and vice-versa.
//! [`Buffer`]s are allocated on a per-[`Device`] basis, and not per
//! [`Channel`]. In order to control which [`Channel`]s to capture in a
//! [`Buffer`], the respective [`Channel`]s must be [enabled][enable_chan] or
//! [disabled][disable_chan].
//!
//! The very first step when working with [`Buffer`]s is to
//! [enable][enable_chan] the capture [`Channel`]s that we want to use, and
//! [disable][disable_chan] those that we don't need. This is done with the
//! functions [`Channel::enable()`] and [`Channel::disable()`]. Note that the
//! [`Channel`]s will really be enabled or disabled when the [`Buffer`]-object
//! is created.
//!
//! Also, not all [`Channel`]s can be enabled. To know whether or not one
//! [`Channel`] can be enabled, use [`Channel::is_scan_element()`].
//!
//! Once the [`Channel`]s have been enabled, and [triggers assigned] (for
//! triggered [`Buffer`]s) the [`Buffer`] object can be created from the
//! [`Device`] object that will be used, with the function
//! [`Device::create_buffer()`]. This call will fail if no [`Channel`]s have
//! been enabled, or for triggered buffers, if the trigger has not been
//! assigned.
//!
//! [`Buffer`] objects are automatically dropped when their scope ends.
//!
//! For additional information on actually working with [`Buffer`]s, including
//! some examples, refer to the [`Buffer` documentation][`Buffer`].
//!
//! Most parts of the documentation for this module were taken from the [libiio
//! documentation](https://analogdevicesinc.github.io/libiio/master/libiio/index.html)
//!
//! [enable_chan]: crate::channel::Channel::enable()
//! [disable_chan]: crate::channel::Channel::disable()
//! [triggers assigned]: crate::device::Device::set_trigger()
#![warn(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

use std::{
    mem, ptr,
    marker::PhantomData,
    os::raw::c_int,
};

use super::*;
use crate::ffi;

/// An Industrial I/O input or output buffer.
///
/// See [here][crate::buffer] for a detailed explanation of how buffers work.
///
/// # Examples
///
#[derive(Debug)]
pub struct Buffer {
    /// The underlying buffer from the C library
    pub(crate) buf: *mut ffi::iio_buffer,
    /// The buffer capacity (# samples from each channel)
    pub(crate) cap: usize,
    // this holds the refcount for libiio
    #[allow(dead_code)]
    pub(crate) ctx: Context,
}

impl Buffer {
    /// Get the buffer size.
    ///
    /// Get the buffer capacity in number of samples from each channel that
    /// the buffer can hold.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Gets a pollable file descriptor for the buffer.
    ///
    /// This can be used to determine when [`Buffer::refill()`] or
    /// [`Buffer::push()`] can be called without blocking.
    pub fn poll_fd(&mut self) -> Result<c_int> {
        let ret = unsafe { ffi::iio_buffer_get_poll_fd(self.buf) };
        sys_result(i32::from(ret), ret)
    }

    /// Make calls to [`Buffer::push()`] or [`Buffer::refill()`] blocking or not.
    ///
    /// A [`Device`] is blocking by default.
    pub fn set_blocking_mode(&mut self, blocking: bool) -> Result<()> {
        let ret = unsafe { ffi::iio_buffer_set_blocking_mode(self.buf, blocking) };
        sys_result(ret, ())
    }

    /// Fetch more samples from the hardware.
    ///
    /// This is only valid for input buffers.
    pub fn refill(&mut self) -> Result<usize> {
        let ret = unsafe { ffi::iio_buffer_refill(self.buf) };
        sys_result(ret as i32, ret as usize)
    }

    /// Send the samples to the hardware.
    ///
    /// This is only valid for output buffers.
    pub fn push(&mut self) -> Result<usize> {
        let ret = unsafe { ffi::iio_buffer_push(self.buf) };
        sys_result(ret as i32, ret as usize)
    }

    /// Send a given number of samples to the hardware.
    ///
    /// This is only valid for output buffers. Note that the number of samples
    /// explicitly doesn't refer to their size in bytes, but the actual number
    /// of samples, regardless of the sample size in memory.
    pub fn push_partial(&mut self, num_samples: usize) -> Result<usize> {
        let ret = unsafe { ffi::iio_buffer_push_partial(self.buf, num_samples) };
        sys_result(ret as i32, ret as usize)
    }

    /// Cancel all buffer operations.
    ///
    /// This function cancels all outstanding [`Buffer`] operations previously
    /// scheduled. This means any pending [`Buffer::push()`] or
    /// [`Buffer::refill()`] operation will abort and return immediately, any
    /// further invocations of these functions on the same buffer will return
    /// immediately with an error.
    ///
    /// Usually [`Buffer::push()`] and [`Buffer::refill()`] will block until
    /// either all data has been transferred or a timeout occurs. This can,
    /// depending on the configuration, take a significant amount of time.
    /// [`Buffer::cancel()`] is useful to bypass these conditions if the
    /// [`Buffer`] operation is supposed to be stopped in response to an
    /// external event (e.g. user input).
    ///
    /// TODO: @fpagliughi, is this true for the rust binding, too?
    /// To be able to capture additional data after calling this function the
    /// buffer should be destroyed and then re-created.
    ///
    /// This function can be called multiple times for the same buffer, but all
    /// but the first invocation will be without additional effect.
    pub fn cancel(&mut self) {
        unsafe {
            ffi::iio_buffer_cancel(self.buf);
        }
    }

    /// Gets an iterator for the data from a channel.
    pub fn channel_iter<T>(&self, chan: &Channel) -> IntoIter<T> {
        unsafe {
            let begin = ffi::iio_buffer_first(self.buf, chan.chan) as *mut T;
            let end = ffi::iio_buffer_end(self.buf) as *const T;
            let ptr = begin;
            let step: isize = ffi::iio_buffer_step(self.buf) / mem::size_of::<T>() as isize;

            IntoIter {
                phantom: PhantomData,
                ptr,
                end,
                step,
            }
        }
    }
}

/// Destroy the buffer when its scope ends.
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { ffi::iio_buffer_destroy(self.buf) }
    }
}

/// An iterator that moves channel data out of a buffer.
#[derive(Debug)]
pub struct IntoIter<T> {
    phantom: PhantomData<T>,
    // Pointer to the current sample for a channel
    ptr: *const T,
    // Pointer to the end of the buffer
    end: *const T,
    // The offset to the next sample for the channel
    step: isize,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        unsafe {
            if self.ptr as *const _ >= self.end {
                None
            }
            else {
                let prev = self.ptr;
                self.ptr = self.ptr.offset(self.step);
                Some(ptr::read(prev))
            }
        }
    }
}
