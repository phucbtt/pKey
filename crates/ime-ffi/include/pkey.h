/*
 * pKey C ABI (spec §13). Used by the macOS input-method shim and the Fcitx5 addon.
 *
 * Threading: one VtEngine must only be used from one thread at a time.
 * Ownership: VtEngine* and VtResult* come from this library and must be released with
 * vt_engine_free / vt_result_free. Text pointers in VtActionView are borrowed from the VtResult
 * and are valid until vt_result_free.
 * Failure: a panic inside the library is caught, returns VT_ERR_PANIC and poisons the engine; a
 * poisoned engine reports "not consumed" for every key (fail open) until it is freed.
 * Compatibility: structs that the caller fills carry struct_size = sizeof(struct). Changing the
 * meaning of anything bumps VT_ABI_VERSION.
 */
#ifndef PKEY_H
#define PKEY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VT_ABI_VERSION 1

typedef struct VtEngine VtEngine;
typedef struct VtResult VtResult;
typedef uint64_t VtContextId;

typedef enum VtStatus {
  VT_OK = 0,
  VT_ERR_NULL = 1,
  VT_ERR_INVALID_UTF8 = 2,
  VT_ERR_CONFIG = 3,
  VT_ERR_UNKNOWN_CONTEXT = 4,
  VT_ERR_ABI_MISMATCH = 5,
  VT_ERR_INVALID_ARG = 6,
  VT_ERR_PANIC = 255
} VtStatus;

typedef enum VtKey {
  VT_KEY_CHAR = 0,
  VT_KEY_BACKSPACE,
  VT_KEY_DELETE,
  VT_KEY_ENTER,
  VT_KEY_TAB,
  VT_KEY_ESCAPE,
  VT_KEY_SPACE,
  VT_KEY_LEFT,
  VT_KEY_RIGHT,
  VT_KEY_UP,
  VT_KEY_DOWN,
  VT_KEY_HOME,
  VT_KEY_END,
  VT_KEY_PAGE_UP,
  VT_KEY_PAGE_DOWN,
  VT_KEY_OTHER
} VtKey;

/* VtKeyEvent.modifiers */
#define VT_MOD_SHIFT 1
#define VT_MOD_CTRL 2
#define VT_MOD_ALT 4
#define VT_MOD_META 8 /* Cmd / Win / Super */
#define VT_MOD_CAPS_LOCK 16
#define VT_MOD_ALT_GR 32

/* VtKeyEvent.flags */
#define VT_FLAG_REPEAT 1
#define VT_FLAG_SYNTHETIC 2
#define VT_FLAG_KEY_UP 4

/* VtContextInfo.capabilities */
#define VT_CAP_PREEDIT (1u << 0)
#define VT_CAP_SELECTION_REPLACE (1u << 1)
#define VT_CAP_SURROUNDING_TEXT (1u << 2)
#define VT_CAP_DELETE_SURROUNDING (1u << 3)
#define VT_CAP_SECURE_INPUT (1u << 4) /* password field: the engine passes everything through */
#define VT_CAP_FOCUS_METADATA (1u << 5)
#define VT_CAP_TERMINAL_HINT (1u << 6)

typedef struct VtKeyEvent {
  uint32_t struct_size; /* sizeof(VtKeyEvent) */
  VtKey key;
  uint32_t ch;            /* Unicode scalar value, 0 if none */
  uint32_t physical_code; /* USB HID usage (page 0x07); modifier keys 0xE0..0xE7 */
  uint16_t modifiers;     /* VT_MOD_* */
  uint8_t flags;          /* VT_FLAG_* */
} VtKeyEvent;

typedef enum VtActionKind {
  VT_ACTION_UPDATE_PREEDIT = 1,
  VT_ACTION_COMMIT = 2,
  VT_ACTION_CANCEL = 3,
  VT_ACTION_MODE_CHANGED = 4
} VtActionKind;

typedef struct VtActionView {
  VtActionKind kind;
  const uint8_t *text_utf8; /* not NUL-terminated; use text_len. NULL for CANCEL / MODE_CHANGED */
  size_t text_len;
  uint32_t caret_utf16;
  uint32_t caret_scalars;
  uint32_t caret_graphemes;
  uint32_t caret_utf8;
  uint32_t mode; /* MODE_CHANGED: 0 = Vietnamese, 1 = English */
} VtActionView;

/* VtContextInfo.app_id_kind */
#define VT_APP_UNKNOWN 0
#define VT_APP_WIN_EXE 1      /* "code.exe" or a full path */
#define VT_APP_MAC_BUNDLE 2   /* "com.apple.TextEdit" */
#define VT_APP_LINUX_PROGRAM 3
#define VT_APP_IBUS_CLIENT 4
#define VT_APP_WIN_AUMID 5

typedef struct VtContextInfo {
  uint32_t struct_size; /* sizeof(VtContextInfo) */
  const uint8_t *app_id_utf8; /* may be NULL */
  size_t app_id_len;
  uint32_t app_id_kind;
  uint32_t capabilities; /* VT_CAP_* */
} VtContextInfo;

/* vt_notify kinds. Focus-in goes through vt_context_update because it carries capabilities. */
#define VT_NOTIFY_FOCUS_OUT 1
#define VT_NOTIFY_CARET_MOVED 2
#define VT_NOTIFY_SELECTION_CHANGED 3
#define VT_NOTIFY_COMPOSITION_TERMINATED_BY_HOST 4
#define VT_NOTIFY_HOST_COMMIT_FAILED 5
#define VT_NOTIFY_RESET 6

/* vt_set_mode */
#define VT_MODE_VIETNAMESE 0
#define VT_MODE_ENGLISH 1

/* vt_result_hotkey: a hotkey other than the mode toggle fired and the adapter has to act. */
#define VT_HOTKEY_NONE 0
#define VT_HOTKEY_CYCLE_SCHEME 1
#define VT_HOTKEY_OPEN_SETTINGS 2

uint32_t vt_abi_version(void);

/* Engine from config.toml text (macros are not loaded). */
VtStatus vt_engine_new(const uint8_t *config_toml, size_t len, VtEngine **out);
/* Engine from the per-user config and macros files, with live reload (vt_engine_poll_reload). */
VtStatus vt_engine_new_from_default_path(VtEngine **out);
void vt_engine_free(VtEngine *e);
VtStatus vt_engine_reload_config(VtEngine *e, const uint8_t *toml, size_t len);
/* Re-read the files if they changed. Returns 1 if new settings were applied, else 0. */
int vt_engine_poll_reload(VtEngine *e);

VtStatus vt_context_create(VtEngine *e, const VtContextInfo *info, VtContextId *out);
void vt_context_destroy(VtEngine *e, VtContextId id);
/* Focus-in: new capabilities and possibly a different application. May return pending actions
 * (a word left over from before is committed). */
VtStatus vt_context_update(VtEngine *e, VtContextId id, const VtContextInfo *info, VtResult **out);

/* 1 if vt_process_key would consume this key, else 0. Does not change any state. */
int vt_would_consume(const VtEngine *e, VtContextId id, const VtKeyEvent *ev);
VtStatus vt_process_key(VtEngine *e, VtContextId id, const VtKeyEvent *ev, VtResult **out);
VtStatus vt_notify(VtEngine *e, VtContextId id, uint32_t notify_kind, VtResult **out);
VtStatus vt_set_mode(VtEngine *e, VtContextId id, uint32_t mode, VtResult **out);
/* Current mode of a context, VT_MODE_*; -1 if the engine or context is unknown. */
int vt_get_mode(const VtEngine *e, VtContextId id);

int vt_result_consumed(const VtResult *r);
uint32_t vt_result_hotkey(const VtResult *r);
size_t vt_result_len(const VtResult *r);
VtStatus vt_result_get(const VtResult *r, size_t i, VtActionView *out);
void vt_result_free(VtResult *r);

#ifdef __cplusplus
}
#endif

#endif /* PKEY_H */
