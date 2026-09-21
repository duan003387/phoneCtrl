// 基于 WebCodecs 的 H.264 播放器：从后端本地 HTTP 流读取长度前缀封装的
// Annex-B access unit，GPU 硬解后绘制到 <canvas>。彻底取代 ffmpeg→MJPEG 链路。

type Nalu = { type: number; data: Uint8Array };

/** 把一段 Annex-B access unit 拆成 NAL 列表（去掉起始码与尾部填充零）。 */
function parseAnnexB(au: Uint8Array): Nalu[] {
  const nalus: Nalu[] = [];
  // 收集每个起始码：body 起始位置 + 下一个起始码开始位置
  const naluStarts: number[] = []; // 首个 NAL 字节下标（起始码之后）
  const scStarts: number[] = []; // 对应起始码首字节下标（用于求上一 NAL 的结束）
  for (let i = 0; i + 2 < au.length; i++) {
    let scLen = 0;
    if (au[i] === 0 && au[i + 1] === 0 && au[i + 2] === 1) {
      scLen = 3;
    } else if (
      i + 3 < au.length &&
      au[i] === 0 &&
      au[i + 1] === 0 &&
      au[i + 2] === 0 &&
      au[i + 3] === 1
    ) {
      scLen = 4;
    }
    if (scLen) {
      scStarts.push(i);
      naluStarts.push(i + scLen);
      i += scLen - 1;
    }
  }
  for (let s = 0; s < naluStarts.length; s++) {
    const begin = naluStarts[s];
    const endRaw = s + 1 < scStarts.length ? scStarts[s + 1] : au.length;
    // 去掉 NAL 末尾的填充零（属于下一起始码的 0x00 前缀）
    let end = endRaw;
    while (end > begin && au[end - 1] === 0) end--;
    if (end - begin < 1) continue;
    const body = au.subarray(begin, end);
    nalus.push({ type: body[0] & 0x1f, data: body });
  }
  return nalus;
}

/** 由 SPS/PPS 构造 avcC（WebCodecs description）。 */
function buildAvcCDescription(sps: Uint8Array, pps: Uint8Array): Uint8Array {
  const len =
    6 + (3 + sps.length) + 1 + (2 + pps.length);
  const d = new Uint8Array(len);
  let i = 0;
  d[i++] = 1; // configurationVersion
  d[i++] = sps[1]; // AVCProfileIndication
  d[i++] = sps[2]; // profile_compatibility
  d[i++] = sps[3]; // AVCLevelIndication
  d[i++] = 0xfc | 3; // lengthSizeMinusOne = 3（4 字节 NAL 长度）
  d[i++] = 0xe0 | 1; // numOfSequenceParameterSets = 1
  d[i++] = (sps.length >> 8) & 0xff;
  d[i++] = sps.length & 0xff;
  d.set(sps, i);
  i += sps.length;
  d[i++] = 1; // numOfPictureParameterSets
  d[i++] = (pps.length >> 8) & 0xff;
  d[i++] = pps.length & 0xff;
  d.set(pps, i);
  return d;
}

function codecString(sps: Uint8Array): string {
  const hex = (b: number) => b.toString(16).padStart(2, "0");
  return `avc1.${hex(sps[1])}${hex(sps[2])}${hex(sps[3])}`;
}

/** 把 NAL 列表转成 AVCC（每个 NAL 前置 4 字节大端长度）。 */
function toAVCC(nalus: Nalu[]): Uint8Array {
  let total = 0;
  for (const n of nalus) total += 4 + n.data.length;
  const out = new Uint8Array(total);
  let i = 0;
  for (const n of nalus) {
    const l = n.data.length;
    out[i++] = (l >>> 24) & 0xff;
    out[i++] = (l >>> 16) & 0xff;
    out[i++] = (l >>> 8) & 0xff;
    out[i++] = l & 0xff;
    out.set(n.data, i);
    i += l;
  }
  return out;
}

export class H264Player {
  private canvas: HTMLCanvasElement;
  private url: string;
  private ctx: CanvasRenderingContext2D;
  private decoder: VideoDecoder | null = null;
  private abort: AbortController | null = null;
  private sps: Uint8Array | null = null;
  private pps: Uint8Array | null = null;
  private configured = false;
  private started = false;
  private frameTs = 0;
  private running = false;
  public onStats?: (fps: number, width: number, height: number) => void;
  public onFirstFrame?: () => void;
  private firstFrameFired = false;
  private frameCount = 0;
  private lastStatsAt = performance.now();

  constructor(canvas: HTMLCanvasElement, url: string) {
    this.canvas = canvas;
    this.url = url;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("canvas 2d 上下文不可用");
    this.ctx = ctx;
  }

  start() {
    this.running = true;
    void this.loop();
  }

  stop() {
    this.running = false;
    this.abort?.abort();
    try {
      this.decoder?.close();
    } catch {
      /* noop */
    }
    this.decoder = null;
    this.configured = false;
    this.sps = this.pps = null;
  }

  private ensureDecoder() {
    if (this.configured || !this.sps || !this.pps) return;
    const description = buildAvcCDescription(this.sps, this.pps);
    this.decoder = new VideoDecoder({
      output: (frame: VideoFrame) => {
        this.render(frame);
        frame.close();
      },
      error: (e) => {
        console.warn("[h264] decoder error, 重置等待关键帧", e);
        this.resetDecoder();
      },
    });
    this.decoder.configure({
      codec: codecString(this.sps),
      description,
      optimizeForLatency: true,
    });
    this.configured = true;
  }

  private resetDecoder() {
    try {
      this.decoder?.close();
    } catch {
      /* noop */
    }
    this.decoder = null;
    this.configured = false;
    this.started = false;
    // 保留 sps/pps，等待下一个关键帧重建
  }

  private render(frame: VideoFrame) {
    const w = frame.displayWidth;
    const h = frame.displayHeight;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.ctx.drawImage(frame, 0, 0, w, h);
    if (!this.firstFrameFired) {
      this.firstFrameFired = true;
      this.onFirstFrame?.();
    }
    this.frameCount++;
    const now = performance.now();
    if (now - this.lastStatsAt >= 1000) {
      const fps = (this.frameCount * 1000) / (now - this.lastStatsAt);
      this.onStats?.(fps, w, h);
      this.frameCount = 0;
      this.lastStatsAt = now;
    }
  }

  private feed(au: Uint8Array) {
    const nalus = parseAnnexB(au);
    // 提取 SPS/PPS 供构造 description；chunk 数据只保留 slice NAL（1=非IDR,5=IDR）。
    let hasIdr = false;
    const slices: Nalu[] = [];
    for (const n of nalus) {
      if (n.type === 7 && !this.sps) this.sps = n.data;
      else if (n.type === 8 && !this.pps) this.pps = n.data;
      if (n.type === 5) hasIdr = true;
      if (n.type === 1 || n.type === 5) slices.push(n);
    }
    // 纯配置包（SPS/PPS/AUD，无 slice）：仅用于取参数，不喂解码器。
    if (slices.length === 0) return;
    if (!this.configured) {
      this.ensureDecoder();
      if (!this.configured) return; // 尚未拿到 SPS/PPS
    }
    // 解码器（重新）配置后必须以 IDR 关键帧起步，否则丢弃直到下一个 IDR。
    if (!this.started) {
      if (!hasIdr) return;
      this.started = true;
    }
    const data = toAVCC(slices);
    try {
      this.decoder!.decode(
        new EncodedVideoChunk({
          type: hasIdr ? "key" : "delta",
          timestamp: this.frameTs,
          data,
        }),
      );
      this.frameTs += 33333;
    } catch (e) {
      console.warn("[h264] decode 失败", e);
      this.resetDecoder();
    }
  }

  private async loop() {
    while (this.running) {
      try {
        this.abort = new AbortController();
        const resp = await fetch(this.url, { signal: this.abort.signal });
        if (!resp.body) throw new Error("无响应体");
        const reader = resp.body.getReader();
        let buf = new Uint8Array(0);
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          if (!value) continue;
          // 追加到缓冲
          const merged = new Uint8Array(buf.length + value.length);
          merged.set(buf, 0);
          merged.set(value, buf.length);
          buf = merged;
          // 解析长度前缀帧：[u32 BE len][AU]
          while (buf.length >= 4) {
            const len =
              ((buf[0] << 24) | (buf[1] << 16) | (buf[2] << 8) | buf[3]) >>> 0;
            if (buf.length < 4 + len) break;
            const au = buf.subarray(4, 4 + len);
            this.feed(au);
            buf = buf.subarray(4 + len);
          }
        }
      } catch (e) {
        if (!this.running) return;
        console.warn("[h264] 流断开，1s 后重连", e);
      }
      if (!this.running) return;
      await new Promise((r) => setTimeout(r, 1000));
      this.resetDecoder();
    }
  }
}
