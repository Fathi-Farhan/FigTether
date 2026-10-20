/**
 * Chunked transfer handling for the bridge socket.
 *
 * The UI owns the only network path, so it also owns the reassembly of a
 * chunked transfer that the sandbox produced and the splitting of a payload
 * that the sandbox handed over. Both directions are bounded: a transfer may
 * not exceed the transfer ceiling and a frame may not exceed the frame cap.
 * The wire shape is the chunk payload of the shared contract, where the final
 * chunk carries the checksum of the whole transfer.
 */

import { sha256Hex } from "@figtether/shared";

import { MAX_FRAME_BYTES, MAX_TRANSFER_BYTES } from "./protocol.js";

/** Result of offering one chunk to the assembler. */
export type ChunkAcceptResult =
  "accepted" | "duplicate" | "out_of_order" | "overflow" | "unknown_transfer";

/** One chunk offered to the assembler. */
export interface InboundChunk {
  readonly sequence: number;
  readonly bytes: Uint8Array;
  readonly checksum: string | null;
}

/** One chunk produced for the wire. */
export interface OutboundChunk {
  readonly transferId: string;
  readonly sequence: number;
  readonly bytes: Uint8Array;
  readonly totalBytes: number;
  readonly checksum: string | null;
}

/**
 * Reassembles one logical transfer.
 *
 * Only one transfer is reassembled at a time. A transfer that is started
 * while another one is open replaces the open transfer, and the partial bytes
 * of the replaced transfer are released.
 */
export class TransferAssembler {
  private transferId: string | null = null;
  private totalBytes = 0;
  private readonly chunks = new Map<number, Uint8Array>();
  private receivedBytes = 0;
  private nextSequence = 0;
  private finalChecksum: string | null = null;

  /** Start a transfer. Returns false when the declared size is out of bounds. */
  begin(transferId: string, totalBytes: number): boolean {
    if (!Number.isFinite(totalBytes) || totalBytes < 0 || totalBytes > MAX_TRANSFER_BYTES) {
      this.reset();
      return false;
    }
    this.transferId = transferId;
    this.totalBytes = Math.trunc(totalBytes);
    this.chunks.clear();
    this.receivedBytes = 0;
    this.nextSequence = 0;
    this.finalChecksum = null;
    return true;
  }

  /** True while a transfer is open. */
  get open(): boolean {
    return this.transferId !== null;
  }

  /** Offer one chunk. */
  accept(chunk: InboundChunk): ChunkAcceptResult {
    if (this.transferId === null) {
      return "unknown_transfer";
    }
    if (chunk.sequence < this.nextSequence) {
      return "duplicate";
    }
    if (chunk.sequence > this.nextSequence) {
      return "out_of_order";
    }
    if (this.receivedBytes + chunk.bytes.length > this.totalBytes) {
      return "overflow";
    }
    this.chunks.set(chunk.sequence, chunk.bytes);
    this.receivedBytes += chunk.bytes.length;
    this.nextSequence = chunk.sequence + 1;
    if (chunk.checksum !== null) {
      this.finalChecksum = chunk.checksum;
    }
    return "accepted";
  }

  /** True when every declared byte arrived. */
  get complete(): boolean {
    return this.transferId !== null && this.receivedBytes === this.totalBytes;
  }

  /**
   * Build the reassembled payload and verify the checksum of the final chunk.
   *
   * Returns null when the transfer is incomplete or the checksum does not
   * match. A mismatch aborts the transfer without publishing a partial result.
   */
  finish(): Uint8Array | null {
    if (!this.complete) {
      return null;
    }
    const out = new Uint8Array(this.totalBytes);
    let offset = 0;
    for (let sequence = 0; sequence < this.nextSequence; sequence += 1) {
      const part = this.chunks.get(sequence);
      if (part === undefined) {
        this.reset();
        return null;
      }
      out.set(part, offset);
      offset += part.length;
    }
    if (this.finalChecksum !== null && sha256Hex(out) !== this.finalChecksum) {
      this.reset();
      return null;
    }
    this.reset();
    return out;
  }

  /** Release every buffer of the open transfer. */
  reset(): void {
    this.transferId = null;
    this.totalBytes = 0;
    this.chunks.clear();
    this.receivedBytes = 0;
    this.nextSequence = 0;
    this.finalChecksum = null;
  }
}

/**
 * Split a payload into frames that respect the frame cap.
 *
 * The checksum covers the whole payload and travels on the final chunk.
 */
export function splitIntoChunks(
  bytes: Uint8Array,
  transferId: string,
  maxChunkBytes: number = MAX_FRAME_BYTES,
): OutboundChunk[] {
  const limit = Math.max(1, Math.min(Math.trunc(maxChunkBytes), MAX_FRAME_BYTES));
  const chunks: OutboundChunk[] = [];
  const total = bytes.length;
  let sequence = 0;
  let offset = 0;
  if (total === 0) {
    return [
      {
        transferId,
        sequence: 0,
        bytes: new Uint8Array(0),
        totalBytes: 0,
        checksum: sha256Hex(new Uint8Array(0)),
      },
    ];
  }
  while (offset < total) {
    const size = Math.min(limit, total - offset);
    const slice = bytes.slice(offset, offset + size);
    const final = offset + size >= total;
    chunks.push({
      transferId,
      sequence,
      bytes: slice,
      totalBytes: total,
      checksum: final ? sha256Hex(bytes) : null,
    });
    offset += size;
    sequence += 1;
  }
  return chunks;
}
