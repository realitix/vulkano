// Copyright (c) 2016 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::cmp;
use std::error;
use std::fmt;

use buffer::TrackedBuffer;
use command_buffer::RawCommandBufferPrototype;
use command_buffer::CommandsList;
use command_buffer::CommandsListSink;
use VulkanObject;
use VulkanPointers;
use vk;

/// Wraps around a commands list and adds at the end of it a command that copies from a buffer to
/// another.
pub struct CmdCopyBufferUnsynced<L, S, D>
    where L: CommandsList, S: TrackedBuffer, D: TrackedBuffer
{
    // Parent commands list.
    previous: L,
    source: S,
    source_raw: vk::Buffer,
    destination: D,
    destination_raw: vk::Buffer,
    src_offset: vk::DeviceSize,
    dst_offset: vk::DeviceSize,
    size: vk::DeviceSize,
}

impl<L, S, D> CmdCopyBufferUnsynced<L, S, D>
    where L: CommandsList, S: TrackedBuffer, D: TrackedBuffer
{
    /// Builds a new command.
    ///
    /// This command will copy from the source to the destination. If their size is not equal, then
    /// the amount of data copied is equal to the smallest of the two.
    ///
    /// # Panic
    ///
    /// - Panics if the source and destination were not created with the same device.
    pub unsafe fn new(previous: L, source: S, destination: D)
                      -> Result<CmdCopyBufferUnsynced<L, S, D>, CmdCopyBufferUnsyncedError>
    {
        // TODO:
        //assert!(previous.is_outside_render_pass());     // TODO: error
        assert_eq!(source.inner().buffer.device().internal_object(),
                   destination.inner().buffer.device().internal_object());

        let (source_raw, src_offset) = {
            let inner = source.inner();
            if !inner.buffer.usage_transfer_src() {
                return Err(CmdCopyBufferUnsyncedError::SourceMissingTransferUsage);
            }
            (inner.buffer.internal_object(), inner.offset)
        };

        let (destination_raw, dst_offset) = {
            let inner = destination.inner();
            if !inner.buffer.usage_transfer_dest() {
                return Err(CmdCopyBufferUnsyncedError::DestinationMissingTransferUsage);
            }
            (inner.buffer.internal_object(), inner.offset)
        };

        let size = cmp::min(source.size(), destination.size());

        if source.conflicts_buffer(0, size, false, &destination, 0, size, true) {
            return Err(CmdCopyBufferUnsyncedError::OverlappingRanges);
        } else {
            debug_assert!(!destination.conflicts_buffer(0, size, true, &source, 0, size, false));
        }

        Ok(CmdCopyBufferUnsynced {
            previous: previous,
            source: source,
            source_raw: source_raw,
            destination: destination,
            destination_raw: destination_raw,
            src_offset: src_offset as u64,
            dst_offset: dst_offset as u64,
            size: size as u64,
        })
    }
}

unsafe impl<L, S, D> CommandsList for CmdCopyBufferUnsynced<L, S, D>
    where L: CommandsList, S: TrackedBuffer, D: TrackedBuffer
{
    #[inline]
    fn append<'a>(&'a self, builder: &mut CommandsListSink<'a>) {
        self.previous.append(builder);

        assert_eq!(self.source.inner().buffer.device().internal_object(),
                   builder.device().internal_object());

        builder.add_buffer_transition(&self.source, 0, self.size as usize, false);
        builder.add_buffer_transition(&self.destination, 0, self.size as usize, true);

        builder.add_command(Box::new(move |raw: &mut RawCommandBufferPrototype| {
            unsafe {
                let vk = raw.device.pointers();
                let cmd = raw.command_buffer.clone().take().unwrap();
                let region = vk::BufferCopy {
                    srcOffset: self.src_offset,
                    dstOffset: self.dst_offset,
                    size: self.size,
                };
                vk.CmdCopyBuffer(cmd, self.source_raw, self.destination_raw, 1, &region);
            }
        }));
    }
}

/// Error that can happen when creating a `CmdCopyBufferUnsynced`.
#[derive(Debug, Copy, Clone)]
pub enum CmdCopyBufferUnsyncedError {
    /// The source buffer is missing the transfer source usage.
    SourceMissingTransferUsage,
    /// The destination buffer is missing the transfer destination usage.
    DestinationMissingTransferUsage,
    /// The source and destination are overlapping.
    OverlappingRanges,
}

impl error::Error for CmdCopyBufferUnsyncedError {
    #[inline]
    fn description(&self) -> &str {
        match *self {
            CmdCopyBufferUnsyncedError::SourceMissingTransferUsage => {
                "the source buffer is missing the transfer source usage"
            },
            CmdCopyBufferUnsyncedError::DestinationMissingTransferUsage => {
                "the destination buffer is missing the transfer destination usage"
            },
            CmdCopyBufferUnsyncedError::OverlappingRanges => {
                "the source and destination are overlapping"
            },
        }
    }
}

impl fmt::Display for CmdCopyBufferUnsyncedError {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}
