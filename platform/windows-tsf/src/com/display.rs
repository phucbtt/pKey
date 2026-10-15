//! Display attribute (the underline under the word being composed), spec §14.1.

use std::cell::Cell;
use std::sync::atomic::Ordering;

use windows::core::{implement, Result, BSTR, GUID};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER};
use windows::Win32::UI::TextServices::{
    IEnumTfDisplayAttributeInfo, IEnumTfDisplayAttributeInfo_Impl, ITfDisplayAttributeInfo,
    ITfDisplayAttributeInfo_Impl, TF_ATTR_INPUT, TF_CT_NONE, TF_DISPLAYATTRIBUTE, TF_LS_SOLID,
};

use super::guids::GUID_DISPLAY_ATTRIBUTE_COMPOSITION;
use super::LIVE_OBJECTS;

fn composition_attribute() -> TF_DISPLAYATTRIBUTE {
    let mut a = TF_DISPLAYATTRIBUTE::default();
    a.crText.r#type = TF_CT_NONE;
    a.crBk.r#type = TF_CT_NONE;
    a.crLine.r#type = TF_CT_NONE;
    a.lsStyle = TF_LS_SOLID;
    a.fBoldLine = false.into();
    a.bAttr = TF_ATTR_INPUT;
    a
}

#[implement(ITfDisplayAttributeInfo)]
pub struct CompositionAttributeInfo;

impl CompositionAttributeInfo {
    fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        CompositionAttributeInfo
    }
}

impl Drop for CompositionAttributeInfo {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl ITfDisplayAttributeInfo_Impl for CompositionAttributeInfo_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_DISPLAY_ATTRIBUTE_COMPOSITION)
    }

    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("pKey composition"))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        if pda.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked for NULL; the caller provides a writable TF_DISPLAYATTRIBUTE.
        unsafe { *pda = composition_attribute() };
        Ok(())
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        // The look is fixed; there is no user customisation.
        Err(windows::Win32::Foundation::E_NOTIMPL.into())
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

#[implement(IEnumTfDisplayAttributeInfo)]
pub struct AttributeEnum {
    /// 0 = nothing returned yet, 1 = the single item was returned.
    pos: Cell<u32>,
}

impl AttributeEnum {
    fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        AttributeEnum { pos: Cell::new(0) }
    }
}

impl Drop for AttributeEnum {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl IEnumTfDisplayAttributeInfo_Impl for AttributeEnum_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        let e = AttributeEnum::new();
        e.pos.set(self.pos.get());
        Ok(e.into())
    }

    fn Next(
        &self,
        count: u32,
        out: *mut Option<ITfDisplayAttributeInfo>,
        fetched: *mut u32,
    ) -> Result<()> {
        if out.is_null() {
            return Err(E_POINTER.into());
        }
        let mut n = 0;
        if count > 0 && self.pos.get() == 0 {
            // SAFETY: `out` has room for at least `count >= 1` elements.
            unsafe { out.write(Some(CompositionAttributeInfo::new().into())) };
            self.pos.set(1);
            n = 1;
        }
        if !fetched.is_null() {
            // SAFETY: checked for NULL.
            unsafe { *fetched = n };
        }
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        self.pos.set(0);
        Ok(())
    }

    fn Skip(&self, count: u32) -> Result<()> {
        if count > 0 {
            self.pos.set(1);
        }
        Ok(())
    }
}

pub(super) fn enum_info() -> IEnumTfDisplayAttributeInfo {
    AttributeEnum::new().into()
}

pub(super) fn info_for(guid: &GUID) -> Result<ITfDisplayAttributeInfo> {
    if *guid == GUID_DISPLAY_ATTRIBUTE_COMPOSITION {
        Ok(CompositionAttributeInfo::new().into())
    } else {
        Err(E_INVALIDARG.into())
    }
}
