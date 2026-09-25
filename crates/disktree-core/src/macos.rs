//! What macOS reports about a volume beyond `statfs`.
//!
//! Finder's name for a volume and the space the system can purge on demand
//! are URL resource values: Foundation computes them, and no system call
//! returns them.

#![allow(
    unsafe_code,
    reason = "Foundation's resource keys are extern statics; each read says \
              why it is sound"
)]

use std::path::Path;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLResourceKey,
    NSURLVolumeAvailableCapacityForImportantUsageKey,
    NSURLVolumeAvailableCapacityKey, NSURLVolumeLocalizedNameKey,
};

type Values = NSDictionary<NSURLResourceKey, AnyObject>;

pub fn volume_name(path: &Path) -> Option<String> {
    // SAFETY: an immutable `NSString` Foundation defines for the life of the
    // process; reading the static has no other effect.
    let key = unsafe { NSURLVolumeLocalizedNameKey };
    // Foundation autoreleases what it returns; off the main run loop nothing
    // drains that until the thread exits.
    autoreleasepool(|_| {
        let values = resource_values(path, &[key])?;
        let name = values.objectForKey(key)?.downcast::<NSString>().ok()?;
        let name = name.to_string();
        (!name.is_empty()).then_some(name)
    })
}

pub fn purgeable_bytes(path: &Path) -> Option<u64> {
    // SAFETY: immutable `NSString`s Foundation defines for the life of the
    // process; reading the statics has no other effect.
    let (important, plain) = unsafe {
        (
            NSURLVolumeAvailableCapacityForImportantUsageKey,
            NSURLVolumeAvailableCapacityKey,
        )
    };
    autoreleasepool(|_| {
        let values = resource_values(path, &[important, plain])?;
        let important = number(&values, important)?;
        let plain = number(&values, plain)?;
        Some(u64::try_from(important.saturating_sub(plain)).unwrap_or(0))
    })
}

fn resource_values(
    path: &Path,
    keys: &[&NSURLResourceKey],
) -> Option<Retained<Values>> {
    let url = NSURL::from_file_path(path)?;
    url.resourceValuesForKeys_error(&NSArray::from_slice(keys))
        .ok()
}

fn number(values: &Values, key: &NSURLResourceKey) -> Option<i64> {
    let value = values.objectForKey(key)?.downcast::<NSNumber>().ok()?;
    Some(value.as_i64())
}
