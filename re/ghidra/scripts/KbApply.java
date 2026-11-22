// Applies the shared KB (functions.tsv, globals.tsv, structs/*.h) to the open program.
// Args: <kb dir>, the merged export of re/db and re/bindings that `kb.py export` writes (re/ghidra/kb).
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.cmd.function.ApplyFunctionSignatureCmd;
import ghidra.app.cmd.function.FunctionRenameOption;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public class KbApply extends GhidraScript {
    static final CategoryPath KB = new CategoryPath("/kb");
    static final CategoryPath OPAQUE = new CategoryPath("/kb/opaque");
    int warnings = 0;
    DataTypeManager dtm;
    Map<String, StructureDataType> pending = new LinkedHashMap<>();

    void warn(String s) { warnings++; println("KBWARN " + s); }

    List<Map<String, String>> readTsv(Path p) throws IOException {
        List<Map<String, String>> rows = new ArrayList<>();
        if (!Files.exists(p)) return rows;
        List<String> lines = Files.readAllLines(p);
        String[] hdr = null;
        for (String line : lines) {
            if (line.isBlank() || line.startsWith("#")) continue;
            String[] c = line.split("\t", -1);
            if (hdr == null) { hdr = c; continue; }
            Map<String, String> m = new HashMap<>();
            for (int i = 0; i < hdr.length; i++) {
                String v = i < c.length ? c[i].trim() : "";
                m.put(hdr[i].trim(), v.equals("-") ? "" : v);
            }
            rows.add(m);
        }
        return rows;
    }

    DataType builtin(String n) {
        switch (n) {
            case "uint8_t": case "u8": case "byte": case "unsigned char": return ByteDataType.dataType;
            case "int8_t": case "i8": case "signed char": return SignedByteDataType.dataType;
            case "char": return CharDataType.dataType;
            case "uint16_t": case "u16": case "unsigned short": return UnsignedShortDataType.dataType;
            case "int16_t": case "i16": case "short": return ShortDataType.dataType;
            case "uint32_t": case "u32": case "unsigned int": case "unsigned": return UnsignedIntegerDataType.dataType;
            case "int32_t": case "i32": case "int": return IntegerDataType.dataType;
            case "uint64_t": case "u64": case "uintptr_t": case "size_t": case "unsigned long long": return UnsignedLongLongDataType.dataType;
            case "int64_t": case "i64": case "intptr_t": case "ptrdiff_t": case "long long": return LongLongDataType.dataType;
            case "float": return FloatDataType.dataType;
            case "double": return DoubleDataType.dataType;
            case "bool": return BooleanDataType.dataType;
            case "void": return VoidDataType.dataType;
            case "wchar_t": return WideChar16DataType.dataType;
        }
        return null;
    }

    DataType named(String n, boolean viaPointer) {
        DataType b = builtin(n);
        if (b != null) return b;
        DataType d = dtm.getDataType(KB, n);
        if (d != null) return d;
        if (!viaPointer) return null;
        DataType o = dtm.getDataType(OPAQUE, n);
        if (o == null) o = dtm.addDataType(new StructureDataType(OPAQUE, n, 0, dtm), DataTypeConflictHandler.KEEP_HANDLER);
        return o;
    }

    // "const T **" / "T*" / "struct T *" -> DataType; null if the base type is unknown and not behind a pointer
    DataType parseType(String t) {
        t = t.replace("const ", "").replace("struct ", "").replace("volatile ", "").trim();
        int stars = 0;
        while (t.endsWith("*")) { stars++; t = t.substring(0, t.length() - 1).trim(); }
        DataType base = named(t, stars > 0);
        if (base == null) return null;
        if (base instanceof VoidDataType && stars == 0) return base;
        DataType d = base;
        for (int i = 0; i < stars; i++) d = new PointerDataType(d, dtm);
        return d;
    }

    static final Pattern FIELD = Pattern.compile(
        "^\\s*/\\*\\s*(0x[0-9a-fA-F]+)\\s*\\*/\\s*([A-Za-z_][A-Za-z0-9_ ]*?[\\s*]+)([A-Za-z_][A-Za-z0-9_]*)\\s*(?:\\[\\s*(0x[0-9a-fA-F]+|\\d+)\\s*\\])?\\s*;\\s*(?://\\s*(.*))?$");
    static final Pattern HDR = Pattern.compile("^\\s*//\\s*(struct|size|subsystem|notes)\\s*:\\s*(.*)$");

    static long num(String s) { return s.startsWith("0x") || s.startsWith("0X") ? Long.parseLong(s.substring(2), 16) : Long.parseLong(s); }

    int applyStructs(Path dir) throws Exception {
        if (!Files.isDirectory(dir)) return 0;
        List<Path> files = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(dir, "*.h")) { for (Path p : ds) files.add(p); }
        Collections.sort(files);
        Map<String, List<String>> bodies = new LinkedHashMap<>();
        Map<String, Integer> sizes = new HashMap<>();
        Map<String, String> notes = new HashMap<>();
        for (Path f : files) {
            String name = null; Integer size = null; String sub = "", note = "";
            List<String> fields = new ArrayList<>();
            for (String line : Files.readAllLines(f)) {
                Matcher h = HDR.matcher(line);
                if (h.matches()) {
                    String k = h.group(1), v = h.group(2).trim();
                    if (k.equals("struct")) name = v.split("\\s+")[0];
                    else if (k.equals("size")) { String s = v.split("\\s+")[0]; if (!s.equals("?")) try { size = (int) num(s); } catch (Exception e) { warn(f.getFileName() + ": bad size " + s); } }
                    else if (k.equals("subsystem")) sub = v;
                    else if (k.equals("notes")) note = v;
                    continue;
                }
                if (FIELD.matcher(line).matches()) fields.add(line);
            }
            if (name == null) { warn(f.getFileName() + ": no '// struct:' header"); continue; }
            bodies.put(name, fields);
            if (size != null) sizes.put(name, size);
            notes.put(name, ("subsystem " + sub + (note.isEmpty() ? "" : "; " + note)).trim());
        }
        // pass 1: placeholders so pointers between KB structs resolve
        for (String n : bodies.keySet())
            if (dtm.getDataType(KB, n) == null)
                dtm.addDataType(new StructureDataType(KB, n, 0, dtm), DataTypeConflictHandler.KEEP_HANDLER);
        // pass 2: by-value struct members need their own layout first; simple dependency-ordered fill
        List<String> order = new ArrayList<>(bodies.keySet());
        Set<String> done = new HashSet<>();
        int guard = 0;
        while (done.size() < order.size() && guard++ < order.size() + 2) {
            for (String n : order) {
                if (done.contains(n)) continue;
                boolean ready = true;
                for (String line : bodies.get(n)) {
                    Matcher m = FIELD.matcher(line); m.matches();
                    String t = m.group(2).trim();
                    if (!t.contains("*")) {
                        String bn = t.replace("const ", "").replace("struct ", "").trim();
                        if (bodies.containsKey(bn) && !bn.equals(n) && !done.contains(bn)) { ready = false; break; }
                    }
                }
                if (!ready && guard <= order.size()) continue;
                fillStruct(n, bodies.get(n), sizes.get(n), notes.get(n));
                done.add(n);
            }
        }
        return done.size();
    }

    void fillStruct(String n, List<String> fields, Integer size, String desc) {
        int max = 0;
        List<Object[]> placed = new ArrayList<>();
        for (String line : fields) {
            Matcher m = FIELD.matcher(line); m.matches();
            int off = (int) num(m.group(1));
            String t = m.group(2).trim(), fname = m.group(3), cnt = m.group(4), cmt = m.group(5) == null ? "" : m.group(5).trim();
            DataType dt = parseType(t);
            if (dt == null) { warn(n + "." + fname + ": unknown type '" + t + "', field skipped"); continue; }
            if (dt instanceof VoidDataType) { warn(n + "." + fname + ": void field skipped"); continue; }
            if (cnt != null) {
                int c = (int) num(cnt);
                if (c <= 0) { warn(n + "." + fname + ": bad count"); continue; }
                dt = new ArrayDataType(dt, c, dt.getLength(), dtm);
            }
            if (dt.getLength() <= 0) { warn(n + "." + fname + ": zero-length type '" + t + "' (define it first), field skipped"); continue; }
            placed.add(new Object[]{off, dt, fname, cmt});
            max = Math.max(max, off + dt.getLength());
        }
        int len = size != null ? size : max;
        if (size != null && max > size) { warn(n + ": fields end at 0x" + Integer.toHexString(max) + " beyond size 0x" + Integer.toHexString(size)); len = max; }
        StructureDataType s = new StructureDataType(KB, n, len, dtm);
        s.setDescription(desc);
        for (Object[] p : placed) {
            try {
                DataType dt = (DataType) p[1];
                s.replaceAtOffset((Integer) p[0], dt, dt.getLength(), (String) p[2], (String) p[3]);
            } catch (Exception e) { warn(n + "." + p[2] + " @0x" + Integer.toHexString((Integer) p[0]) + ": " + e.getMessage()); }
        }
        DataType existing = dtm.getDataType(KB, n);
        if (existing instanceof Structure) ((Structure) existing).replaceWith(s);
        else dtm.addDataType(s, DataTypeConflictHandler.REPLACE_HANDLER);
    }

    Address addr(String s) {
        return toAddr(Long.parseUnsignedLong(s.toLowerCase().replace("0x", ""), 16));
    }

    boolean ensureFunc(Address a) throws Exception {
        Function f = getFunctionAt(a);
        if (f != null && f.getBody().getNumAddresses() > 1) return false;
        if (currentProgram.getListing().getInstructionAt(a) == null)
            new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
        if (f == null) {
            new CreateFunctionCmd(a).applyTo(currentProgram, monitor);
            return getFunctionAt(a) != null;
        }
        CreateFunctionCmd.fixupFunctionBody(currentProgram, f, monitor);
        return false;
    }

    void clearNameElsewhere(String name, Address keep) {
        for (Symbol s : currentProgram.getSymbolTable().getGlobalSymbols(name)) {
            if (s.getAddress().equals(keep)) continue;
            try {
                if (s.getSymbolType() == SymbolType.FUNCTION) s.setName("KBOLD_" + name + "_" + s.getAddress(), SourceType.ANALYSIS);
                else s.delete();
                warn("name " + name + " was on " + s.getAddress() + ", moved");
            } catch (Exception e) { warn("cannot free name " + name + ": " + e.getMessage()); }
        }
    }

    // "<ret> [conv] name(<type> <arg>, ...)" -> FunctionDefinitionDataType, null on failure
    FunctionDefinitionDataType parseProto(String proto, String name) {
        Matcher m = Pattern.compile("^(.*?)\\b([A-Za-z_][A-Za-z0-9_]*)\\s*\\((.*)\\)\\s*;?\\s*$").matcher(proto.trim());
        if (!m.matches()) return null;
        String ret = m.group(1).replaceAll("__fastcall|__cdecl|__stdcall|__thiscall", "").trim();
        DataType rt = parseType(ret.isEmpty() ? "void" : ret);
        if (rt == null) return null;
        FunctionDefinitionDataType fd = new FunctionDefinitionDataType(name, dtm);
        try { fd.setCallingConvention("__fastcall"); } catch (Exception e) {}
        fd.setReturnType(rt);
        List<ParameterDefinition> ps = new ArrayList<>();
        String args = m.group(3).trim();
        if (!args.isEmpty() && !args.equals("void")) {
            int i = 0;
            for (String a : args.split(",")) {
                a = a.trim();
                if (a.equals("...")) { fd.setVarArgs(true); continue; }
                Matcher am = Pattern.compile("^(.*?[\\s*])([A-Za-z_][A-Za-z0-9_]*)$").matcher(a);
                String t, n;
                if (am.matches() && builtin(am.group(2)) == null && !am.group(1).trim().isEmpty()) { t = am.group(1).trim(); n = am.group(2); }
                else { t = a; n = "a" + (i + 1); }
                DataType dt = parseType(t);
                if (dt == null) return null;
                ps.add(new ParameterDefinitionImpl(n, dt, null));
                i++;
            }
        }
        fd.setArguments(ps.toArray(new ParameterDefinition[0]));
        return fd;
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Path kb = Paths.get(args.length > 0 ? args[0] : "re/ghidra/kb");
        dtm = currentProgram.getDataTypeManager();
        int structs = applyStructs(kb.resolve("structs"));

        int nf = 0, renamed = 0, created = 0, protos = 0, reverted = 0;
        SymbolTable st = currentProgram.getSymbolTable();
        List<Map<String, String>> frows = readTsv(kb.resolve("functions.tsv")), grows = readTsv(kb.resolve("globals.tsv"));
        Set<Address> kbAddrs = new HashSet<>();
        for (Map<String, String> r : frows) if (!r.getOrDefault("addr", "").isEmpty()) try { kbAddrs.add(addr(r.get("addr"))); } catch (Exception e) {}
        for (Map<String, String> r : grows) if (!r.getOrDefault("addr", "").isEmpty()) try { kbAddrs.add(addr(r.get("addr"))); } catch (Exception e) {}
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            String c = f.getComment();
            if (c == null || !c.startsWith("KB ") || kbAddrs.contains(f.getEntryPoint())) continue;
            f.setComment(null);
            f.setName(SymbolUtilities.getDefaultFunctionName(f.getEntryPoint()), SourceType.DEFAULT);
            reverted++;
        }
        for (Address a : currentProgram.getListing().getCommentAddressIterator(CommentType.PLATE, currentProgram.getMemory(), true)) {
            String c = currentProgram.getListing().getComment(CommentType.PLATE, a);
            if (c == null || !c.startsWith("KB ") || kbAddrs.contains(a) || getFunctionAt(a) != null) continue;
            currentProgram.getListing().setComment(a, CommentType.PLATE, null);
            for (Symbol s : st.getSymbols(a)) if (s.getSource() == SourceType.USER_DEFINED && s.getSymbolType() == SymbolType.LABEL) s.delete();
            reverted++;
        }
        for (Map<String, String> r : frows) {
            String a = r.getOrDefault("addr", ""), name = r.getOrDefault("name", "");
            if (a.isEmpty() || name.isEmpty()) continue;
            nf++;
            try {
                Address ad = addr(a);
                if (ensureFunc(ad)) created++;
                Function f = getFunctionAt(ad);
                if (f == null) { warn("no function at " + a + " (" + name + ")"); continue; }
                if (!f.getName().equals(name)) {
                    clearNameElsewhere(name, ad);
                    f.setName(name, SourceType.USER_DEFINED);
                    renamed++;
                }
                String proto = r.getOrDefault("proto", "");
                if (!proto.isEmpty()) {
                    FunctionDefinitionDataType fd = parseProto(proto, name);
                    if (fd == null) warn("proto not parsed for " + name + ": " + proto);
                    else {
                        ApplyFunctionSignatureCmd cmd = new ApplyFunctionSignatureCmd(ad, fd, SourceType.USER_DEFINED, false, false,
                            DataTypeConflictHandler.DEFAULT_HANDLER, FunctionRenameOption.NO_CHANGE);
                        if (cmd.applyTo(currentProgram, monitor)) protos++; else warn("proto apply failed for " + name + ": " + cmd.getStatusMsg());
                    }
                }
                String plate = "KB " + r.getOrDefault("level", "?") + " " + r.getOrDefault("subsystem", "?")
                    + (r.getOrDefault("sig", "").isEmpty() ? "" : " sig=" + r.get("sig"))
                    + " | " + r.getOrDefault("notes", "") + " | " + r.getOrDefault("evidence", "");
                if (!plate.equals(f.getComment())) f.setComment(plate);
            } catch (Exception e) { warn("function " + a + " " + name + ": " + e); }
        }

        int ng = 0;
        for (Map<String, String> r : grows) {
            String a = r.getOrDefault("addr", ""), name = r.getOrDefault("name", "");
            if (a.isEmpty() || name.isEmpty()) continue;
            ng++;
            try {
                Address ad = addr(a);
                Symbol prim = st.getPrimarySymbol(ad);
                if (prim == null || !prim.getName().equals(name)) {
                    clearNameElsewhere(name, ad);
                    Symbol s = st.createLabel(ad, name, SourceType.USER_DEFINED);
                    s.setPrimary();
                    renamed++;
                }
                String t = r.getOrDefault("type", "");
                if (t.matches("code|vtable|void|label")) t = "";
                Matcher vm = Pattern.compile("^vtable\\s*\\[\\s*(0x[0-9a-fA-F]+|\\d+)\\s*\\]$").matcher(t);
                if (vm.matches()) t = "void *[" + vm.group(1) + "]";
                if (!t.isEmpty() && currentProgram.getListing().getInstructionContaining(ad) == null) {
                    DataType dt = null;
                    Matcher am = Pattern.compile("^(.*?)\\s*\\[\\s*(0x[0-9a-fA-F]+|\\d+)\\s*\\]$").matcher(t);
                    if (am.matches()) {
                        DataType el = parseType(am.group(1));
                        if (el != null && el.getLength() > 0) dt = new ArrayDataType(el, (int) num(am.group(2)), el.getLength(), dtm);
                    } else dt = parseType(t);
                    if (dt == null || dt.getLength() <= 0) warn("global " + name + ": type '" + t + "' left unapplied");
                    else {
                        Data d = currentProgram.getListing().getDataAt(ad);
                        if (d == null || !d.getDataType().isEquivalent(dt))
                            DataUtilities.createData(currentProgram, ad, dt, -1, DataUtilities.ClearDataMode.CLEAR_ALL_UNDEFINED_CONFLICT_DATA);
                    }
                }
                String cm = "KB " + r.getOrDefault("level", "?") + " " + r.getOrDefault("subsystem", "?") + " | " + r.getOrDefault("notes", "") + " | " + r.getOrDefault("evidence", "");
                if (!cm.equals(currentProgram.getListing().getComment(CommentType.PLATE, ad)))
                    currentProgram.getListing().setComment(ad, CommentType.PLATE, cm);
            } catch (Exception e) { warn("global " + a + " " + name + ": " + e); }
        }
        println(String.format("KBAPPLY functions=%d renamed=%d created=%d protos=%d globals=%d structs=%d reverted=%d warnings=%d",
            nf, renamed, created, protos, ng, structs, reverted, warnings));
    }
}
