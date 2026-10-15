use windows::core::GUID;

/// COM class id of the text service.
pub const CLSID_PKEY: GUID = GUID::from_u128(0x434E620C_D5E7_4347_B6BC_BE6BE00C417D);
/// The one input profile (Vietnamese keyboard) the service registers.
pub const GUID_PROFILE_VIETNAMESE: GUID = GUID::from_u128(0xC66FB435_3A56_46FD_BA71_C033C1FC7EBD);
/// Display attribute used to underline the composition.
pub const GUID_DISPLAY_ATTRIBUTE_COMPOSITION: GUID =
    GUID::from_u128(0x35B7A9A7_B56C_4713_AF01_977C81D4FE7B);

/// vi-VN
pub const LANGID_VIETNAMESE: u16 = 0x042A;
pub const PROFILE_DESCRIPTION: &str = "pKey";
pub const CLSID_STRING: &str = "{434E620C-D5E7-4347-B6BC-BE6BE00C417D}";
/// Global compartment that carries the shared Vietnamese/English state between processes
/// (0 = Vietnamese, 1 = English). A private guid: the system's `KEYBOARD_OPENCLOSE` compartment
/// would also change what Windows' own indicator shows.
pub const GUID_COMPARTMENT_MODE: GUID = GUID::from_u128(0x7B1E2A94_6C3D_4F58_9A07_5D8E3C12B6F0);
