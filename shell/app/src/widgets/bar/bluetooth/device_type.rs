#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BluetoothDeviceKind {
    Computer,
    Phone,
    Modem,
    NetworkWireless,
    AudioHeadset,
    AudioHeadphones,
    CameraVideo,
    AudioCard,
    InputGaming,
    InputKeyboard,
    InputTablet,
    InputMouse,
    Printer,
    CameraPhoto,
    Unknown,
    VideoDisplay,
    MultimediaPlayer,
    Scanner,
}

impl BluetoothDeviceKind {
    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Phone => "phone",
            Self::Modem => "modem",
            Self::NetworkWireless => "network-wireless",
            Self::AudioHeadset => "headset_mic",
            Self::AudioHeadphones => "headphones",
            Self::CameraVideo => "camera-video",
            Self::AudioCard => "media_bluetooth_on",
            Self::InputGaming => "input-gaming",
            Self::InputKeyboard => "keyboard",
            Self::InputTablet => "trackpad_input",
            Self::InputMouse => "mouse",
            Self::Printer => "printer",
            Self::CameraPhoto => "camera-photo",
            Self::Unknown => "unknown",
            Self::VideoDisplay => "video-display",
            Self::MultimediaPlayer => "multimedia-player",
            Self::Scanner => "scanner",
        }
    }
}

const COD_MAJOR_DEVICE_MASK: u32 = 0x1f00;
const COD_MAJOR_DEVICE_SHIFT: u8 = 8;
const COD_MINOR_AUDIO_MASK: u32 = 0xfc;
const COD_MINOR_AUDIO_SHIFT: u8 = 2;
const COD_MINOR_PERIPH_TYPE_MASK: u32 = 0xc0;
const COD_MINOR_PERIPH_TYPE_SHIFT: u8 = 6;
const COD_MINOR_PERIPH_SUBTYPE_MASK: u32 = 0x1e;
const COD_MINOR_PERIPH_SUBTYPE_SHIFT: u8 = 2;
const COD_IMAGING_PRINTER_BIT: u32 = 0x80;
const COD_IMAGING_CAMERA_BIT: u32 = 0x20;

const GAP_APPEARANCE_CATEGORY_MASK: u32 = 0xffc0;
const GAP_APPEARANCE_CATEGORY_SHIFT: u8 = 6;
const GAP_APPEARANCE_SUBCATEGORY_MASK: u32 = 0x3f;
const GAP_APPEARANCE_HID_GENERIC_CATEGORY: u32 = 0x0f;

pub(super) fn device_kind(
    device_class: Option<u32>,
    appearance: Option<u32>,
) -> BluetoothDeviceKind {
    device_class
        .and_then(parse_class)
        .or_else(|| appearance.and_then(parse_appearance))
        .unwrap_or(BluetoothDeviceKind::Unknown)
}

fn parse_class(device_class: u32) -> Option<BluetoothDeviceKind> {
    let major = (device_class & COD_MAJOR_DEVICE_MASK) >> COD_MAJOR_DEVICE_SHIFT;

    match major {
        0x01 => Some(BluetoothDeviceKind::Computer),
        0x02 => parse_phone_class(device_class),
        0x03 => Some(BluetoothDeviceKind::NetworkWireless),
        0x04 => Some(parse_audio_class(device_class)),
        0x05 => parse_peripheral_class(device_class),
        0x06 => parse_imaging_class(device_class),
        _ => None,
    }
}

fn parse_phone_class(device_class: u32) -> Option<BluetoothDeviceKind> {
    match (device_class & COD_MINOR_AUDIO_MASK) >> COD_MINOR_AUDIO_SHIFT {
        0x01 | 0x02 | 0x03 | 0x05 => Some(BluetoothDeviceKind::Phone),
        0x04 => Some(BluetoothDeviceKind::Modem),
        _ => None,
    }
}

fn parse_audio_class(device_class: u32) -> BluetoothDeviceKind {
    match (device_class & COD_MINOR_AUDIO_MASK) >> COD_MINOR_AUDIO_SHIFT {
        0x01 | 0x02 => BluetoothDeviceKind::AudioHeadset,
        0x06 => BluetoothDeviceKind::AudioHeadphones,
        0x0b..=0x0d => BluetoothDeviceKind::CameraVideo,
        _ => BluetoothDeviceKind::AudioCard,
    }
}

fn parse_peripheral_class(device_class: u32) -> Option<BluetoothDeviceKind> {
    let peripheral_type =
        (device_class & COD_MINOR_PERIPH_TYPE_MASK) >> COD_MINOR_PERIPH_TYPE_SHIFT;
    let peripheral_subtype =
        (device_class & COD_MINOR_PERIPH_SUBTYPE_MASK) >> COD_MINOR_PERIPH_SUBTYPE_SHIFT;

    match peripheral_type {
        0x00 if matches!(peripheral_subtype, 0x01 | 0x02) => Some(BluetoothDeviceKind::InputGaming),
        0x01 => Some(BluetoothDeviceKind::InputKeyboard),
        0x02 if peripheral_subtype == 0x05 => Some(BluetoothDeviceKind::InputTablet),
        0x02 => Some(BluetoothDeviceKind::InputMouse),
        _ => None,
    }
}

fn parse_imaging_class(device_class: u32) -> Option<BluetoothDeviceKind> {
    if device_class & COD_IMAGING_PRINTER_BIT != 0 {
        Some(BluetoothDeviceKind::Printer)
    } else if device_class & COD_IMAGING_CAMERA_BIT != 0 {
        Some(BluetoothDeviceKind::CameraPhoto)
    } else {
        None
    }
}

fn parse_appearance(appearance: u32) -> Option<BluetoothDeviceKind> {
    match (appearance & GAP_APPEARANCE_CATEGORY_MASK) >> GAP_APPEARANCE_CATEGORY_SHIFT {
        0x00 => Some(BluetoothDeviceKind::Unknown),
        0x01 => Some(BluetoothDeviceKind::Phone),
        0x02 => Some(BluetoothDeviceKind::Computer),
        0x05 => Some(BluetoothDeviceKind::VideoDisplay),
        0x0a => Some(BluetoothDeviceKind::MultimediaPlayer),
        0x0b => Some(BluetoothDeviceKind::Scanner),
        GAP_APPEARANCE_HID_GENERIC_CATEGORY => match appearance & GAP_APPEARANCE_SUBCATEGORY_MASK {
            0x01 => Some(BluetoothDeviceKind::InputKeyboard),
            0x02 => Some(BluetoothDeviceKind::InputMouse),
            0x03 | 0x04 => Some(BluetoothDeviceKind::InputGaming),
            0x05 => Some(BluetoothDeviceKind::InputTablet),
            0x08 => Some(BluetoothDeviceKind::Scanner),
            _ => None,
        },
        _ => None,
    }
}
