#![no_std]
#![feature(type_alias_impl_trait)]

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::String;

pub mod display;
pub mod ntp;
pub mod websocket;
pub mod wifi;

pub static DISPLAY_CHANNEL: Channel<CriticalSectionRawMutex, String<256>, 3> = Channel::new();
