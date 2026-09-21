import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * sendevent 注入桥：直接向 /dev/input/eventX 写入内核 input_event 结构，
 * 绕开 IInputManager / DisplayManager 等框架层注入（三星 One UI 8 等机型会
 * 静默丢弃经虚拟显示器转发的框架注入事件，但内核 input 子系统照常接收）。
 *
 * 这是 `adb shell input` 命令底层做的事，但本桥常驻运行，省去每次 fork
 * sendevent 进程 + adb 往返的 ~100ms 开销，实现实时拖拽。
 *
 * 命令行参数：
 *   argv[0] = 触摸设备节点（如 /dev/input/event3），留空则自动探测
 *
 * stdin 协议（大端）：
 *   0x01 down:   [x u32][y u32]
 *   0x02 move:   [x u32][y u32]
 *   0x03 up:     [x u32][y u32]
 *   0x04 quit:   （无负载）
 *
 * 坐标为设备物理像素（0..screenW, 0..screenH），本桥按 ABS 轴 min/max 线性映射。
 */
public final class InputBridge {

    // 内核 input 事件码
    private static final int EV_SYN = 0x00;
    private static final int EV_KEY = 0x01;
    private static final int EV_ABS = 0x03;
    private static final int SYN_REPORT = 0x00;
    private static final int BTN_TOUCH = 0x14a;          // 330
    private static final int ABS_MT_SLOT = 0x2f;         // 47
    private static final int ABS_MT_TRACKING_ID = 0x39;  // 57
    private static final int ABS_MT_POSITION_X = 0x35;   // 53
    private static final int ABS_MT_POSITION_Y = 0x36;   // 54

    private static String devPath;
    private static int eventSize = 24;   // input_event 字节数（64 位 24，32 位 16）
    private static int absXMin, absXMax, absYMin, absYMax;
    private static int screenW = 1080, screenH = 1920;
    private static int trackingId = -1;

    private static byte[] evBuf;
    private static BufferedOutputStream out;

    public static void main(String[] args) throws Exception {
        readScreenSize();
        Probe probe = (args.length > 0 && !args[0].isEmpty())
                ? probeNode(args[0])
                : autoDetect();
        if (probe == null || probe.dev == null) {
            System.err.println("BRIDGE_ERR no-touch-device");
            System.exit(2);
            return;
        }
        devPath = probe.dev;
        absXMin = probe.xMin; absXMax = probe.xMax;
        absYMin = probe.yMin; absYMax = probe.yMax;
        ensureRange();

        detectEventSize();
        out = new BufferedOutputStream(new FileOutputStream(devPath), 8192);
        System.err.println("BRIDGE_READY dev=" + devPath + " size=" + eventSize
                + " absX=" + absXMin + ":" + absXMax + " absY=" + absYMin + ":" + absYMax
                + " screen=" + screenW + "x" + screenH);

        DataInputStream in = new DataInputStream(new BufferedInputStream(System.in, 8192));
        while (true) {
            int type;
            try {
                type = in.readUnsignedByte();
            } catch (Exception e) {
                break; // stdin EOF：流结束，退出
            }
            switch (type) {
                case 0x01: { int x = in.readInt(); int y = in.readInt(); sendTouch(1, x, y); break; }
                case 0x02: { int x = in.readInt(); int y = in.readInt(); sendTouch(2, x, y); break; }
                case 0x03: { int x = in.readInt(); int y = in.readInt(); sendTouch(3, x, y); break; }
                case 0x04: out.flush(); return;
                default: return;
            }
        }
    }

    private static void sendTouch(int action, int px, int py) throws Exception {
        int ax = mapAxis(px, absXMin, absXMax, screenW);
        int ay = mapAxis(py, absYMin, absYMax, screenH);
        if (action == 1) {           // down
            trackingId = (trackingId + 1) & 0xFFFF;
            writeEvent(EV_ABS, ABS_MT_SLOT, 0);
            writeEvent(EV_ABS, ABS_MT_TRACKING_ID, trackingId);
            writeEvent(EV_ABS, ABS_MT_POSITION_X, ax);
            writeEvent(EV_ABS, ABS_MT_POSITION_Y, ay);
            writeEvent(EV_KEY, BTN_TOUCH, 1);
            writeEvent(EV_SYN, SYN_REPORT, 0);
        } else if (action == 2) {    // move
            writeEvent(EV_ABS, ABS_MT_SLOT, 0);
            writeEvent(EV_ABS, ABS_MT_POSITION_X, ax);
            writeEvent(EV_ABS, ABS_MT_POSITION_Y, ay);
            writeEvent(EV_SYN, SYN_REPORT, 0);
        } else {                     // up
            writeEvent(EV_ABS, ABS_MT_SLOT, 0);
            writeEvent(EV_ABS, ABS_MT_TRACKING_ID, -1);
            writeEvent(EV_KEY, BTN_TOUCH, 0);
            writeEvent(EV_SYN, SYN_REPORT, 0);
            trackingId = -1;
        }
        out.flush();
    }

    private static int mapAxis(int px, int min, int max, int screen) {
        if (screen <= 1 || max <= min) return clamp(px, min, max);
        int v = (int) Math.round((double) px / (screen - 1) * (max - min)) + min;
        return clamp(v, min, max);
    }

    private static int clamp(int v, int lo, int hi) {
        return v < lo ? lo : (v > hi ? hi : v);
    }

    private static void writeEvent(int type, int code, int value) throws Exception {
        ByteBuffer b = ByteBuffer.wrap(evBuf).order(ByteOrder.LITTLE_ENDIAN);
        if (eventSize == 24) {
            b.putLong(0L);   // tv_sec
            b.putLong(0L);   // tv_nsec（内核用接收时刻覆盖）
        } else {
            b.putInt(0);     // tv_sec (32-bit)
            b.putInt(0);     // tv_usec
        }
        b.putShort((short) type);
        b.putShort((short) code);
        b.putInt(value);
        out.write(evBuf);
    }

    /** 探测 64/32 位，决定 input_event 结构大小。 */
    private static void detectEventSize() {
        boolean is64 = true;
        try {
            String abi = System.getProperty("os.arch", "");
            if (!abi.contains("64")) {
                is64 = System.getProperty("sun.arch.data.model", "64").contains("64");
            }
        } catch (Exception ignored) {
        }
        eventSize = is64 ? 24 : 16;
        evBuf = new byte[eventSize];
    }

    private static final class Probe {
        String dev;
        int xMin, xMax, yMin, yMax;
    }

    /** 执行 `getevent -p`，找同时具备 ABS_MT_POSITION_X/Y 的触摸设备并读量程。 */
    private static Probe autoDetect() {
        return parseGetevent(run("getevent", "-p"), null);
    }

    /** 已知节点时，仅从 getevent -p 提取该节点的量程。 */
    private static Probe probeNode(String node) {
        Probe p = parseGetevent(run("getevent", "-p"), node);
        if (p == null) {
            p = new Probe();
            p.dev = node;
        }
        return p;
    }

    /**
     * 解析 getevent -p。wanted==null 时自动挑选首个含 X/Y 轴的设备；
     * 否则只匹配指定节点。
     */
    private static Probe parseGetevent(String output, String wanted) {
        Probe result = null;
        Probe cur = null;
        Pattern devP = Pattern.compile("add device \\d+:\\s*(/dev/input/\\S+)");
        Pattern absP = Pattern.compile(
                "([0-9a-fA-F]{4})\\s*:\\s*value.*min\\s+(-?\\d+).*max\\s+(-?\\d+)");
        for (String line : output.split("\\r?\\n")) {
            Matcher m = devP.matcher(line);
            if (m.find()) {
                // 结算上一台
                if (cur != null && isUsable(cur) && (result == null || wanted == null)) {
                    if (wanted == null || cur.dev.endsWith(wanted)) result = cur;
                }
                cur = new Probe();
                cur.dev = m.group(1);
                continue;
            }
            if (cur == null) continue;
            Matcher a = absP.matcher(line);
            if (a.find()) {
                int code = Integer.parseInt(a.group(1), 16);
                int mn = Integer.parseInt(a.group(2));
                int mx = Integer.parseInt(a.group(3));
                if (code == ABS_MT_POSITION_X) { cur.xMin = mn; cur.xMax = mx; }
                else if (code == ABS_MT_POSITION_Y) { cur.yMin = mn; cur.yMax = mx; }
            }
        }
        if (cur != null && isUsable(cur) && (result == null || wanted == null)) {
            if (wanted == null || cur.dev.endsWith(wanted)) result = cur;
        }
        return result;
    }

    private static boolean isUsable(Probe p) {
        return p.dev != null && p.xMax > p.xMin && p.yMax > p.yMin;
    }

    private static void ensureRange() {
        if (absXMax <= absXMin) { absXMin = 0; absXMax = Math.max(1, screenW - 1); }
        if (absYMax <= absYMin) { absYMin = 0; absYMax = Math.max(1, screenH - 1); }
    }

    private static void readScreenSize() {
        try {
            String s = run("wm", "size");
            Matcher m = Pattern.compile("(\\d+)x(\\d+)").matcher(s);
            String last = null;
            while (m.find()) last = m.group(0); // 取最后一个（Physical 后可能有 Override）
            if (last != null) {
                int i = last.indexOf('x');
                screenW = Integer.parseInt(last.substring(0, i));
                screenH = Integer.parseInt(last.substring(i + 1));
            }
        } catch (Exception ignored) {
        }
    }

    private static String run(String... cmd) {
        try {
            Process p = new ProcessBuilder(cmd).redirectErrorStream(false).start();
            byte[] o = readAll(p.getInputStream());
            p.waitFor();
            return new String(o);
        } catch (Exception e) {
            return "";
        }
    }

    private static byte[] readAll(java.io.InputStream is) throws Exception {
        java.io.ByteArrayOutputStream bos = new java.io.ByteArrayOutputStream();
        byte[] buf = new byte[4096];
        int n;
        while ((n = is.read(buf)) > 0) bos.write(buf, 0, n);
        return bos.toByteArray();
    }
}
