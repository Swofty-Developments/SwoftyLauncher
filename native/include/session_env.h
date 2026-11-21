#pragma once
#include <stddef.h>
#include <string.h>

/* swofty\session_env.json: the session's environment for a game the EA app starts, written by the CLI
   (apps/cli/src/stores/ea/session_env.rs) and read by the loader at attach. Plain C that also compiles as C++, with
   no Windows calls, so the unit test runs on the host.

   {"schema": 1, "request": "<launch request id>", "written": <unix s>, "expires": <unix s>,
    "env": {"NAME": "value", ...}}

   The file holds for a game process created from `written` (less SWOFTY_SESSION_ENV_SKEW) to `expires`. */

#define SWOFTY_SESSION_ENV_SCHEMA 1
#define SWOFTY_SESSION_ENV_MAX 256
#define SWOFTY_SESSION_ENV_SKEW 30LL
#define SWOFTY_SESSION_ENV_MAX_LIFE 3600LL
#define SWOFTY_SESSION_ENV_DEPTH 16

typedef struct {
    long long schema, written, expires;
    const char *request;
    int count;
    const char *names[SWOFTY_SESSION_ENV_MAX];
    const char *values[SWOFTY_SESSION_ENV_MAX];
} SwoftySessionEnv;

enum {
    SWOFTY_SESSION_ENV_OK,
    SWOFTY_SESSION_ENV_UNREADABLE,
    SWOFTY_SESSION_ENV_OTHER_SCHEMA,
    SWOFTY_SESSION_ENV_NO_REQUEST,
    SWOFTY_SESSION_ENV_BAD_WINDOW,
    SWOFTY_SESSION_ENV_EXPIRED,
    SWOFTY_SESSION_ENV_LATER,
};

static inline const char *swofty_session_env_why(int code)
{
    switch (code) {
    case SWOFTY_SESSION_ENV_OK:
        return "valid";
    case SWOFTY_SESSION_ENV_UNREADABLE:
        return "it is not JSON in the layout the launcher writes";
    case SWOFTY_SESSION_ENV_OTHER_SCHEMA:
        return "it has a schema this loader does not read";
    case SWOFTY_SESSION_ENV_NO_REQUEST:
        return "it names no launch request";
    case SWOFTY_SESSION_ENV_BAD_WINDOW:
        return "its time window is not one the launcher writes";
    case SWOFTY_SESSION_ENV_EXPIRED:
        return "it expired before this game started";
    case SWOFTY_SESSION_ENV_LATER:
        return "it was written after this game started, for another start";
    }
    return "unknown";
}

static inline void swofty_session_env_ws(char **p)
{
    while (**p == ' ' || **p == '\t' || **p == '\r' || **p == '\n')
        (*p)++;
}

static inline int swofty_session_env_hex4(const char *s, unsigned long *out)
{
    unsigned long v = 0;
    for (int i = 0; i < 4; i++) {
        char c = s[i];
        int d = c >= '0' && c <= '9' ? c - '0' : c >= 'a' && c <= 'f' ? c - 'a' + 10 : c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
        if (d < 0)
            return 0;
        v = v << 4 | (unsigned long)d;
    }
    *out = v;
    return 1;
}

static inline void swofty_session_env_utf8(char **out, unsigned long cp)
{
    char *o = *out;
    if (cp < 0x80) {
        *o++ = (char)cp;
    } else if (cp < 0x800) {
        *o++ = (char)(0xc0 | cp >> 6);
        *o++ = (char)(0x80 | (cp & 0x3f));
    } else if (cp < 0x10000) {
        *o++ = (char)(0xe0 | cp >> 12);
        *o++ = (char)(0x80 | (cp >> 6 & 0x3f));
        *o++ = (char)(0x80 | (cp & 0x3f));
    } else {
        *o++ = (char)(0xf0 | cp >> 18);
        *o++ = (char)(0x80 | (cp >> 12 & 0x3f));
        *o++ = (char)(0x80 | (cp >> 6 & 0x3f));
        *o++ = (char)(0x80 | (cp & 0x3f));
    }
    *out = o;
}

/* Decodes the JSON string *p starts at in place, as UTF-8 ending in a NUL, and moves *p past it. A decoded string is
   never longer than its source, so the text only shrinks. NULL for a malformed string or one holding \u0000. */
static inline char *swofty_session_env_string(char **p)
{
    char *in = *p;
    if (*in != '"')
        return NULL;
    char *start = ++in, *out = start;
    for (;;) {
        unsigned char c = (unsigned char)*in;
        if (c < 0x20)
            return NULL;
        if (c == '"') {
            in++;
            break;
        }
        if (c != '\\') {
            *out++ = *in++;
            continue;
        }
        in++;
        switch (*in) {
        case '"':
        case '\\':
        case '/':
            *out++ = *in++;
            break;
        case 'b':
            *out++ = '\b';
            in++;
            break;
        case 'f':
            *out++ = '\f';
            in++;
            break;
        case 'n':
            *out++ = '\n';
            in++;
            break;
        case 'r':
            *out++ = '\r';
            in++;
            break;
        case 't':
            *out++ = '\t';
            in++;
            break;
        case 'u': {
            unsigned long cp, lo;
            if (!swofty_session_env_hex4(in + 1, &cp))
                return NULL;
            in += 5;
            if (cp >= 0xd800 && cp < 0xdc00) {
                if (in[0] != '\\' || in[1] != 'u' || !swofty_session_env_hex4(in + 2, &lo) || lo < 0xdc00 || lo > 0xdfff)
                    return NULL;
                in += 6;
                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
            } else if (cp >= 0xdc00 && cp <= 0xdfff) {
                return NULL;
            }
            if (!cp)
                return NULL;
            swofty_session_env_utf8(&out, cp);
            break;
        }
        default:
            return NULL;
        }
    }
    *out = 0;
    *p = in;
    return start;
}

static inline int swofty_session_env_int(char **p, long long *out)
{
    char *s = *p;
    int neg = *s == '-';
    if (neg)
        s++;
    if (*s < '0' || *s > '9')
        return 0;
    long long v = 0;
    while (*s >= '0' && *s <= '9') {
        if (v > (9223372036854775807LL - (*s - '0')) / 10)
            return 0;
        v = v * 10 + (*s++ - '0');
    }
    if (*s == '.' || *s == 'e' || *s == 'E')
        return 0;
    *out = neg ? -v : v;
    *p = s;
    return 1;
}

static inline int swofty_session_env_skip(char **p, int depth)
{
    char *s = *p;
    if (depth > SWOFTY_SESSION_ENV_DEPTH)
        return 0;
    if (*s == '"')
        return swofty_session_env_string(p) != NULL;
    if (*s == '{' || *s == '[') {
        char close = *s == '{' ? '}' : ']';
        s++;
        swofty_session_env_ws(&s);
        if (*s == close) {
            *p = s + 1;
            return 1;
        }
        for (;;) {
            swofty_session_env_ws(&s);
            if (close == '}') {
                if (!swofty_session_env_string(&s))
                    return 0;
                swofty_session_env_ws(&s);
                if (*s++ != ':')
                    return 0;
                swofty_session_env_ws(&s);
            }
            if (!swofty_session_env_skip(&s, depth + 1))
                return 0;
            swofty_session_env_ws(&s);
            if (*s == ',') {
                s++;
                continue;
            }
            if (*s++ != close)
                return 0;
            *p = s;
            return 1;
        }
    }
    if (!strncmp(s, "true", 4) || !strncmp(s, "null", 4)) {
        *p = s + 4;
        return 1;
    }
    if (!strncmp(s, "false", 5)) {
        *p = s + 5;
        return 1;
    }
    if (*s != '-' && (*s < '0' || *s > '9'))
        return 0;
    while ((*s >= '0' && *s <= '9') || *s == '-' || *s == '+' || *s == '.' || *s == 'e' || *s == 'E')
        s++;
    *p = s;
    return 1;
}

static inline int swofty_session_env_valid_name(const char *name) { return *name && !strchr(name, '='); }

static inline int swofty_session_env_pairs(char **p, SwoftySessionEnv *e)
{
    char *s = *p;
    if (*s++ != '{')
        return 0;
    swofty_session_env_ws(&s);
    if (*s == '}') {
        *p = s + 1;
        return 1;
    }
    for (;;) {
        swofty_session_env_ws(&s);
        char *name = swofty_session_env_string(&s);
        if (!name || !swofty_session_env_valid_name(name))
            return 0;
        swofty_session_env_ws(&s);
        if (*s++ != ':')
            return 0;
        swofty_session_env_ws(&s);
        char *value = swofty_session_env_string(&s);
        if (!value || e->count >= SWOFTY_SESSION_ENV_MAX)
            return 0;
        e->names[e->count] = name;
        e->values[e->count] = value;
        e->count++;
        swofty_session_env_ws(&s);
        if (*s == ',') {
            s++;
            continue;
        }
        if (*s++ != '}')
            return 0;
        *p = s;
        return 1;
    }
}

/* Reads the file's text, which it decodes in place: the names and values in *e point into it. 0 when it is not
   the layout above. */
static inline int swofty_session_env_parse(char *text, SwoftySessionEnv *e)
{
    memset(e, 0, sizeof *e);
    e->schema = e->written = e->expires = -1;
    char *p = text;
    if ((unsigned char)p[0] == 0xef && (unsigned char)p[1] == 0xbb && (unsigned char)p[2] == 0xbf)
        p += 3;
    swofty_session_env_ws(&p);
    if (*p++ != '{')
        return 0;
    swofty_session_env_ws(&p);
    if (*p == '}') {
        p++;
    } else {
        for (;;) {
            swofty_session_env_ws(&p);
            char *key = swofty_session_env_string(&p);
            if (!key)
                return 0;
            swofty_session_env_ws(&p);
            if (*p++ != ':')
                return 0;
            swofty_session_env_ws(&p);
            int ok;
            if (!strcmp(key, "schema"))
                ok = swofty_session_env_int(&p, &e->schema);
            else if (!strcmp(key, "written"))
                ok = swofty_session_env_int(&p, &e->written);
            else if (!strcmp(key, "expires"))
                ok = swofty_session_env_int(&p, &e->expires);
            else if (!strcmp(key, "request"))
                ok = (e->request = swofty_session_env_string(&p)) != NULL;
            else if (!strcmp(key, "env"))
                ok = !e->count && swofty_session_env_pairs(&p, e);
            else
                ok = swofty_session_env_skip(&p, 0);
            if (!ok)
                return 0;
            swofty_session_env_ws(&p);
            if (*p == ',') {
                p++;
                continue;
            }
            if (*p++ != '}')
                return 0;
            break;
        }
    }
    swofty_session_env_ws(&p);
    return *p == 0;
}

/* Whether a parsed file holds for a game process created at `started` (unix seconds). */
static inline int swofty_session_env_check(const SwoftySessionEnv *e, long long started)
{
    if (e->schema != SWOFTY_SESSION_ENV_SCHEMA)
        return SWOFTY_SESSION_ENV_OTHER_SCHEMA;
    if (!e->request || !*e->request)
        return SWOFTY_SESSION_ENV_NO_REQUEST;
    if (e->written <= 0 || e->expires <= e->written || e->expires - e->written > SWOFTY_SESSION_ENV_MAX_LIFE)
        return SWOFTY_SESSION_ENV_BAD_WINDOW;
    if (started > e->expires)
        return SWOFTY_SESSION_ENV_EXPIRED;
    if (started + SWOFTY_SESSION_ENV_SKEW < e->written)
        return SWOFTY_SESSION_ENV_LATER;
    return SWOFTY_SESSION_ENV_OK;
}

/* Reads `text` and checks it for a game process created at `started`: SWOFTY_SESSION_ENV_OK, or why it doesn't
   hold. */
static inline int swofty_session_env_read(char *text, long long started, SwoftySessionEnv *e)
{
    if (!swofty_session_env_parse(text, e))
        return SWOFTY_SESSION_ENV_UNREADABLE;
    return swofty_session_env_check(e, started);
}
