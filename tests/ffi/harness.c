/* C harness for the pKey C ABI (spec 13.2). Exit code 0 = all checks passed.
 * Built by crates/ime-ffi/tests/c_harness.rs with the system C compiler against the cdylib,
 * optionally under ASan/UBSan. */
#include <stdio.h>
#include <string.h>

#include "pkey.h"

static int failures = 0;

#define CHECK(cond)                                                          \
  do {                                                                       \
    if (!(cond)) {                                                           \
      fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond); \
      failures++;                                                            \
    }                                                                        \
  } while (0)

static VtKeyEvent key(VtKey k, uint32_t ch) {
  VtKeyEvent ev;
  memset(&ev, 0, sizeof ev);
  ev.struct_size = sizeof ev;
  ev.key = k;
  ev.ch = ch;
  return ev;
}

/* Types `s`; copies the text of the last COMMIT seen into `out`. */
static void type_text(VtEngine *e, VtContextId ctx, const char *s, char *out, size_t cap) {
  out[0] = 0;
  for (; *s; s++) {
    VtKeyEvent ev = key(*s == ' ' ? VT_KEY_SPACE : VT_KEY_CHAR, (uint32_t)(unsigned char)*s);
    VtResult *r = NULL;
    CHECK(vt_process_key(e, ctx, &ev, &r) == VT_OK);
    if (!r) continue;
    for (size_t i = 0; i < vt_result_len(r); i++) {
      VtActionView a;
      CHECK(vt_result_get(r, i, &a) == VT_OK);
      if (a.kind == VT_ACTION_COMMIT && a.text_len < cap) {
        memcpy(out, a.text_utf8, a.text_len);
        out[a.text_len] = 0;
      }
    }
    vt_result_free(r);
  }
}

int main(void) {
  CHECK(vt_abi_version() == VT_ABI_VERSION);

  VtEngine *e = NULL;
  CHECK(vt_engine_new(NULL, 0, &e) == VT_OK);
  CHECK(e != NULL);

  VtContextInfo info;
  memset(&info, 0, sizeof info);
  info.struct_size = sizeof info;
  info.capabilities = VT_CAP_PREEDIT;
  VtContextId ctx = 0;
  CHECK(vt_context_create(e, &info, &ctx) == VT_OK);

  char out[64];
  type_text(e, ctx, "tieengs vieejt ", out, sizeof out);
  /* The last commit is "viet" with circumflex and dot below: 76 69 e1 bb 87 74. */
  CHECK(strcmp(out, "vi\xe1\xbb\x87t") == 0);

  /* would_consume does not change state: a letter is consumed, a space with no word is not. */
  VtKeyEvent a = key(VT_KEY_CHAR, 'a');
  CHECK(vt_would_consume(e, ctx, &a) == 1);
  VtKeyEvent sp = key(VT_KEY_SPACE, ' ');
  CHECK(vt_would_consume(e, ctx, &sp) == 0);

  /* Mode switch: English passes keys through. */
  VtResult *r = NULL;
  CHECK(vt_set_mode(e, ctx, VT_MODE_ENGLISH, &r) == VT_OK);
  CHECK(vt_result_len(r) == 1);
  vt_result_free(r);
  CHECK(vt_get_mode(e, ctx) == VT_MODE_ENGLISH);
  r = NULL;
  CHECK(vt_process_key(e, ctx, &a, &r) == VT_OK);
  CHECK(vt_result_consumed(r) == 0);
  vt_result_free(r);

  /* Error handling. */
  r = NULL;
  CHECK(vt_process_key(e, ctx, NULL, &r) == VT_ERR_NULL);
  CHECK(vt_process_key(e, 424242, &a, &r) == VT_ERR_UNKNOWN_CONTEXT);
  VtKeyEvent bad = a;
  bad.struct_size = 1;
  CHECK(vt_process_key(e, ctx, &bad, &r) == VT_ERR_ABI_MISMATCH);
  const char *broken = "[general\n";
  VtEngine *e2 = NULL;
  CHECK(vt_engine_new((const uint8_t *)broken, strlen(broken), &e2) == VT_ERR_CONFIG);
  CHECK(e2 == NULL);

  /* Freeing NULL is allowed. */
  vt_result_free(NULL);
  vt_engine_free(NULL);

  vt_context_destroy(e, ctx);
  vt_engine_free(e);

  if (failures) {
    fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  puts("ok");
  return 0;
}
