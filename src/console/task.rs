// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::UsbDriver;
use defmt_rtt as _;
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::EndpointError;
use embassy_usb::UsbDevice;

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo(class: &mut CdcAcmClass<'static, UsbDriver>) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        class.write_packet(data).await?;
    }
}

#[embassy_executor::task]
pub async fn usb_task(mut device: UsbDevice<'static, UsbDriver>) {
    defmt::info!("Starting usb task");
    device.run().await;
}

#[embassy_executor::task]
pub async fn echo_task(mut class: CdcAcmClass<'static, UsbDriver>) {
    loop {
        defmt::debug!("Waiting for USB");
        class.wait_connection().await;
        defmt::debug!("USB connected");
        let _ = echo(&mut class).await;
        defmt::debug!("USB disconnected");
    }
}
