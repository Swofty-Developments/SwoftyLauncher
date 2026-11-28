using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Swofty.Overcooked2.Core
{
    internal sealed class JsonException : Exception
    {
        internal JsonException(string message)
            : base(message)
        {
        }
    }

    internal static class Json
    {
        internal static object Parse(string text)
        {
            if (text == null)
            {
                throw new JsonException("no JSON text");
            }
            Reader reader = new Reader(text);
            reader.SkipSpace();
            object value = reader.ReadValue();
            reader.SkipSpace();
            if (!reader.AtEnd)
            {
                throw reader.Error("text after the JSON value");
            }
            return value;
        }

        internal static string Quote(string s)
        {
            if (s == null)
            {
                return "null";
            }
            StringBuilder sb = new StringBuilder(s.Length + 2);
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"':
                        sb.Append("\\\"");
                        break;
                    case '\\':
                        sb.Append("\\\\");
                        break;
                    case '\n':
                        sb.Append("\\n");
                        break;
                    case '\r':
                        sb.Append("\\r");
                        break;
                    case '\t':
                        sb.Append("\\t");
                        break;
                    case '\b':
                        sb.Append("\\b");
                        break;
                    case '\f':
                        sb.Append("\\f");
                        break;
                    default:
                        if (c < 0x20)
                        {
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        }
                        else
                        {
                            sb.Append(c);
                        }
                        break;
                }
            }
            sb.Append('"');
            return sb.ToString();
        }

        private sealed class Reader
        {
            private readonly string text;
            private int pos;

            internal Reader(string text)
            {
                this.text = text;
                if (text.Length > 0 && text[0] == '﻿')
                {
                    pos = 1;
                }
            }

            internal bool AtEnd
            {
                get { return pos >= text.Length; }
            }

            internal JsonException Error(string what)
            {
                int line = 1;
                int col = 1;
                for (int i = 0; i < pos && i < text.Length; i++)
                {
                    if (text[i] == '\n')
                    {
                        line++;
                        col = 1;
                    }
                    else
                    {
                        col++;
                    }
                }
                return new JsonException(what + " at line " + line + ", column " + col);
            }

            internal void SkipSpace()
            {
                while (pos < text.Length)
                {
                    char c = text[pos];
                    if (c == ' ' || c == '\t' || c == '\n' || c == '\r')
                    {
                        pos++;
                    }
                    else
                    {
                        break;
                    }
                }
            }

            internal object ReadValue()
            {
                if (AtEnd)
                {
                    throw Error("unexpected end of JSON");
                }
                char c = text[pos];
                switch (c)
                {
                    case '{':
                        return ReadObject();
                    case '[':
                        return ReadArray();
                    case '"':
                        return ReadString();
                    case 't':
                        Expect("true");
                        return true;
                    case 'f':
                        Expect("false");
                        return false;
                    case 'n':
                        Expect("null");
                        return null;
                    default:
                        if (c == '-' || (c >= '0' && c <= '9'))
                        {
                            return ReadNumber();
                        }
                        throw Error("unexpected character '" + c + "'");
                }
            }

            private void Expect(string word)
            {
                if (string.CompareOrdinal(text, pos, word, 0, word.Length) != 0)
                {
                    throw Error("expected " + word);
                }
                pos += word.Length;
            }

            private Dictionary<string, object> ReadObject()
            {
                Dictionary<string, object> obj = new Dictionary<string, object>();
                pos++;
                SkipSpace();
                if (!AtEnd && text[pos] == '}')
                {
                    pos++;
                    return obj;
                }
                while (true)
                {
                    SkipSpace();
                    if (AtEnd || text[pos] != '"')
                    {
                        throw Error("expected a member name");
                    }
                    string key = ReadString();
                    SkipSpace();
                    if (AtEnd || text[pos] != ':')
                    {
                        throw Error("expected ':'");
                    }
                    pos++;
                    SkipSpace();
                    obj[key] = ReadValue();
                    SkipSpace();
                    if (AtEnd)
                    {
                        throw Error("unterminated object");
                    }
                    if (text[pos] == ',')
                    {
                        pos++;
                        continue;
                    }
                    if (text[pos] == '}')
                    {
                        pos++;
                        return obj;
                    }
                    throw Error("expected ',' or '}'");
                }
            }

            private List<object> ReadArray()
            {
                List<object> list = new List<object>();
                pos++;
                SkipSpace();
                if (!AtEnd && text[pos] == ']')
                {
                    pos++;
                    return list;
                }
                while (true)
                {
                    SkipSpace();
                    list.Add(ReadValue());
                    SkipSpace();
                    if (AtEnd)
                    {
                        throw Error("unterminated array");
                    }
                    if (text[pos] == ',')
                    {
                        pos++;
                        continue;
                    }
                    if (text[pos] == ']')
                    {
                        pos++;
                        return list;
                    }
                    throw Error("expected ',' or ']'");
                }
            }

            private string ReadString()
            {
                pos++;
                StringBuilder sb = new StringBuilder();
                while (true)
                {
                    if (AtEnd)
                    {
                        throw Error("unterminated string");
                    }
                    char c = text[pos++];
                    if (c == '"')
                    {
                        return sb.ToString();
                    }
                    if (c != '\\')
                    {
                        if (c < 0x20)
                        {
                            pos--;
                            throw Error("control character in a string");
                        }
                        sb.Append(c);
                        continue;
                    }
                    if (AtEnd)
                    {
                        throw Error("unterminated escape");
                    }
                    char e = text[pos++];
                    switch (e)
                    {
                        case '"':
                            sb.Append('"');
                            break;
                        case '\\':
                            sb.Append('\\');
                            break;
                        case '/':
                            sb.Append('/');
                            break;
                        case 'b':
                            sb.Append('\b');
                            break;
                        case 'f':
                            sb.Append('\f');
                            break;
                        case 'n':
                            sb.Append('\n');
                            break;
                        case 'r':
                            sb.Append('\r');
                            break;
                        case 't':
                            sb.Append('\t');
                            break;
                        case 'u':
                            if (pos + 4 > text.Length)
                            {
                                throw Error("short \\u escape");
                            }
                            int code;
                            if (!int.TryParse(text.Substring(pos, 4), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out code))
                            {
                                throw Error("bad \\u escape");
                            }
                            sb.Append((char)code);
                            pos += 4;
                            break;
                        default:
                            pos--;
                            throw Error("bad escape '\\" + e + "'");
                    }
                }
            }

            private double ReadNumber()
            {
                int start = pos;
                if (text[pos] == '-')
                {
                    pos++;
                }
                while (pos < text.Length)
                {
                    char c = text[pos];
                    if ((c >= '0' && c <= '9') || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-')
                    {
                        pos++;
                    }
                    else
                    {
                        break;
                    }
                }
                double value;
                if (!double.TryParse(text.Substring(start, pos - start), NumberStyles.Float, CultureInfo.InvariantCulture, out value))
                {
                    pos = start;
                    throw Error("bad number");
                }
                return value;
            }
        }
    }

    internal sealed class JsonWriter
    {
        private readonly StringBuilder sb = new StringBuilder();
        private readonly Stack<bool> first = new Stack<bool>();
        private bool pendingValueAfterName;

        internal JsonWriter BeginObject()
        {
            Separator();
            sb.Append('{');
            first.Push(true);
            return this;
        }

        internal JsonWriter EndObject()
        {
            first.Pop();
            sb.Append('}');
            return this;
        }

        internal JsonWriter BeginArray()
        {
            Separator();
            sb.Append('[');
            first.Push(true);
            return this;
        }

        internal JsonWriter EndArray()
        {
            first.Pop();
            sb.Append(']');
            return this;
        }

        internal JsonWriter Name(string name)
        {
            Separator();
            sb.Append(Json.Quote(name)).Append(": ");
            pendingValueAfterName = true;
            return this;
        }

        internal JsonWriter Value(string s)
        {
            Separator();
            sb.Append(Json.Quote(s));
            return this;
        }

        internal JsonWriter Value(bool b)
        {
            Separator();
            sb.Append(b ? "true" : "false");
            return this;
        }

        private void Separator()
        {
            if (pendingValueAfterName)
            {
                pendingValueAfterName = false;
                return;
            }
            if (first.Count == 0)
            {
                return;
            }
            if (!first.Peek())
            {
                sb.Append(", ");
            }
            first.Pop();
            first.Push(false);
        }

        public override string ToString()
        {
            return sb.ToString();
        }
    }
}
