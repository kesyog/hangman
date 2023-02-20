use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::peripheral::AdvertiseError;
use nrf_softdevice::ble::{gatt_server, Connection, GattValue};
use nrf_softdevice::{ble, raw as raw_sd, Softdevice};
use zerocopy::{AsBytes, FromBytes};

#[rustfmt::skip]
const ADVERTISING_DATA: &[u8] = &[
    2,
    raw_sd::BLE_GAP_AD_TYPE_FLAGS as u8,
    (raw_sd::BLE_GAP_ADV_FLAG_LE_GENERAL_DISC_MODE | raw_sd::BLE_GAP_ADV_FLAG_BR_EDR_NOT_SUPPORTED) as u8,
    16,
    raw_sd::BLE_GAP_AD_TYPE_COMPLETE_LOCAL_NAME as u8,
    b'P', b'r', b'o', b'g', b'r', b'e', b's', b's', b'o', b'r', b'_', b'1', b'7', b'1', b'9',
];

#[rustfmt::skip]
const SCAN_RESPONSE_DATA: &[u8] = &[
    17,
    raw_sd::BLE_GAP_AD_TYPE_128BIT_SERVICE_UUID_COMPLETE as u8,
    0x57, 0xad, 0xfe, 0x4f, 0xd3, 0x13, 0xcc, 0x9d, 0xc9, 0x40, 0xa6, 0x1e, 0x01, 0x17, 0x4e, 0x7e,
];

pub enum DataOpcode {
    BatteryVoltage(u32),
    Weight(f32, u32),
    LowPowerWarning,
}

impl DataOpcode {
    fn opcode(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..) => 0x00,
            DataOpcode::Weight(..) => 0x01,
            DataOpcode::LowPowerWarning => 0x04,
        }
    }

    fn length(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..) => 4,
            DataOpcode::Weight(..) => 8,
            DataOpcode::LowPowerWarning => 0,
        }
    }

    fn value(&self) -> [u8; 8] {
        let mut value = [0; 8];
        match self {
            DataOpcode::BatteryVoltage(voltage) => {
                value[0..4].copy_from_slice(&voltage.to_le_bytes())
            }
            DataOpcode::Weight(weight, timestamp) => {
                value[0..4].copy_from_slice(&weight.to_le_bytes());
                value[4..].copy_from_slice(&timestamp.to_le_bytes());
            }
            DataOpcode::LowPowerWarning => (),
        };
        value
    }
}

#[derive(AsBytes)]
#[repr(C, packed)]
pub struct DataPoint {
    opcode: u8,
    length: u8,
    value: [u8; 8],
}

impl From<DataOpcode> for DataPoint {
    fn from(opcode: DataOpcode) -> Self {
        Self {
            opcode: opcode.opcode(),
            length: opcode.length(),
            value: opcode.value(),
        }
    }
}

impl GattValue for DataPoint {
    const MIN_SIZE: usize = 2;
    const MAX_SIZE: usize = 2;

    fn from_gatt(data: &[u8]) -> Self {
        if data.len() < 2 {
            panic!("DataPoint is too small");
        }
        let mut value = [0; 8];
        let length = usize::min(data.len() - 2, data[1] as usize).min(value.len());
        value[0..length].copy_from_slice(&data[2..2 + length]);
        Self {
            opcode: data[0],
            length: length as u8,
            value,
        }
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &self.as_bytes()[..length.into()]
    }
}

#[derive(defmt::Format)]
pub enum ControlOpcode {
    Tare = 0x64,
    StartMeasurement = 0x65,
    StopMeasurement = 0x66,
    Shutdown = 0x6E,
    SampleBattery = 0x6F,
}

#[derive(FromBytes, AsBytes)]
#[repr(C, packed)]
pub struct ControlPoint {
    opcode: u8,
    length: u8,
}

impl From<ControlOpcode> for ControlPoint {
    fn from(opcode: ControlOpcode) -> Self {
        Self {
            opcode: opcode as u8,
            length: 0,
        }
    }
}

impl TryFrom<ControlPoint> for ControlOpcode {
    type Error = u8;

    fn try_from(value: ControlPoint) -> Result<Self, Self::Error> {
        match value.opcode {
            0x64 => Ok(ControlOpcode::Tare),
            0x65 => Ok(ControlOpcode::StartMeasurement),
            0x66 => Ok(ControlOpcode::StopMeasurement),
            0x6E => Ok(ControlOpcode::Shutdown),
            0x6F => Ok(ControlOpcode::SampleBattery),
            other => Err(other),
        }
    }
}

impl GattValue for ControlPoint {
    const MIN_SIZE: usize = 2;
    const MAX_SIZE: usize = 2;

    fn from_gatt(data: &[u8]) -> Self {
        Self::read_from(data).unwrap()
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &self.as_bytes()[..length.into()]
    }
}

#[nrf_softdevice::gatt_service(uuid = "7e4e1701-1ea6-40c9-9dcc-13d34ffead57")]
pub struct ProgressorService {
    #[characteristic(uuid = "7e4e1702-1ea6-40c9-9dcc-13d34ffead57", notify)]
    data: DataPoint,

    #[characteristic(
        uuid = "7e4e1703-1ea6-40c9-9dcc-13d34ffead57",
        write,
        write_without_response
    )]
    control: ControlPoint,
}

#[nrf_softdevice::gatt_server]
pub struct Server {
    pub progressor: ProgressorService,
}

// not really gatt. oops
pub async fn advertise(sd: &Softdevice) -> Result<Connection, AdvertiseError> {
    let config = ble::peripheral::Config::default();
    let adv = ble::peripheral::ConnectableAdvertisement::ScannableUndirected {
        adv_data: ADVERTISING_DATA,
        scan_data: SCAN_RESPONSE_DATA,
    };
    ble::peripheral::advertise_connectable(sd, adv, &config).await
}

// Just for testing
#[embassy_executor::task]
pub async fn adv_task(sd: &'static Softdevice, server: Server) {
    loop {
        let conn = advertise(sd).await.unwrap();
        defmt::println!("Connected");

        let res = gatt_server::run(&conn, &server, |e| match e {
            ServerEvent::Progressor(e) => match e {
                ProgressorServiceEvent::ControlWrite(val) => {
                    defmt::info!("ControlWrite: {:?}", ControlOpcode::try_from(val));
                }
                ProgressorServiceEvent::DataCccdWrite { notifications } => {
                    defmt::info!("DataCccdWrite: {}", notifications);
                }
            },
        })
        .await;

        if let Err(e) = res {
            defmt::info!("gatt_server exited with error: {:?}", e);
        }
    }
}
