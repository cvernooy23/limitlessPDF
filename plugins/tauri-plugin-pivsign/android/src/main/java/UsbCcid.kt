package com.plugin.pivsign

import android.hardware.usb.UsbConstants
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbEndpoint
import android.hardware.usb.UsbInterface
import android.hardware.usb.UsbManager
import com.yubico.yubikit.core.Transport
import com.yubico.yubikit.core.smartcard.SmartCardConnection
import java.io.IOException

/**
 * A minimal USB CCID (Chip Card Interface Device, USB class 0x0B) transport that
 * satisfies YubiKit's [SmartCardConnection], so a physical PIV card (e.g. a CAC)
 * in a USB-C smart-card reader can be driven by YubiKit's tested PIV layer.
 *
 * Implements just enough of the CCID bulk protocol: IccPowerOn (ATR) and
 * XfrBlock (APDU exchange), with time-extension handling. APDU-level chaining
 * (GET RESPONSE / command chaining) is left to YubiKit's SmartCardProtocol.
 */
class UsbCcidConnection private constructor(
  private val connection: UsbDeviceConnection,
  private val iface: UsbInterface,
  private val bulkIn: UsbEndpoint,
  private val bulkOut: UsbEndpoint,
) : SmartCardConnection {

  private var seq: Int = 0
  private val atr: ByteArray

  init {
    atr = powerOn()
  }

  override fun getTransport(): Transport = Transport.USB

  // Contact PIV readers/cards generally support extended-length APDUs, which
  // PivSession needs for RSA-2048 GENERAL AUTHENTICATE. If a specific reader or
  // card does not, this is the first thing to revisit.
  override fun isExtendedLengthApduSupported(): Boolean = true

  override fun getAtr(): ByteArray = atr

  @Synchronized
  override fun sendAndReceive(apdu: ByteArray): ByteArray = xfrBlock(apdu)

  override fun close() {
    try {
      powerOff()
    } catch (_: Exception) {
    }
    try {
      connection.releaseInterface(iface)
    } finally {
      connection.close()
    }
  }

  // ── CCID message framing ────────────────────────────────────────────────

  private fun nextSeq(): Int {
    val s = seq
    seq = (seq + 1) and 0xFF
    return s
  }

  /** Build a PC_to_RDR message: 10-byte header + payload. */
  private fun message(messageType: Int, payload: ByteArray, param: ByteArray): ByteArray {
    val msg = ByteArray(10 + payload.size)
    msg[0] = messageType.toByte()
    val len = payload.size
    msg[1] = (len and 0xFF).toByte()
    msg[2] = ((len ushr 8) and 0xFF).toByte()
    msg[3] = ((len ushr 16) and 0xFF).toByte()
    msg[4] = ((len ushr 24) and 0xFF).toByte()
    msg[5] = 0 // bSlot
    msg[6] = nextSeq().toByte()
    msg[7] = param[0]
    msg[8] = param[1]
    msg[9] = param[2]
    System.arraycopy(payload, 0, msg, 10, payload.size)
    return msg
  }

  private fun writeOut(msg: ByteArray) {
    var offset = 0
    while (offset < msg.size) {
      val chunk = minOf(bulkOut.maxPacketSize, msg.size - offset)
      val part = msg.copyOfRange(offset, offset + chunk)
      val sent = connection.bulkTransfer(bulkOut, part, part.size, TIMEOUT_MS)
      if (sent < 0) throw IOException("USB write failed")
      offset += sent
    }
  }

  /** Read one RDR_to_PC message, returning its data payload (after the header). */
  private fun readReply(): ByteArray {
    while (true) {
      val buf = ByteArray(READ_BUF)
      val out = java.io.ByteArrayOutputStream()
      // Read the 10-byte header first.
      var headerLen = 0
      while (headerLen < 10) {
        val n = connection.bulkTransfer(bulkIn, buf, buf.size, TIMEOUT_MS)
        if (n < 0) throw IOException("USB read failed (header)")
        out.write(buf, 0, n)
        headerLen = out.size()
      }
      var data = out.toByteArray()
      val dwLength =
        (data[1].toInt() and 0xFF) or
          ((data[2].toInt() and 0xFF) shl 8) or
          ((data[3].toInt() and 0xFF) shl 16) or
          ((data[4].toInt() and 0xFF) shl 24)
      // Keep reading until we have the full message.
      while (data.size < 10 + dwLength) {
        val n = connection.bulkTransfer(bulkIn, buf, buf.size, TIMEOUT_MS)
        if (n < 0) throw IOException("USB read failed (body)")
        out.write(buf, 0, n)
        data = out.toByteArray()
      }
      val bStatus = data[7].toInt() and 0xFF
      val commandStatus = (bStatus ushr 6) and 0x03
      if (commandStatus == 2) {
        // Time extension requested — read the next block.
        continue
      }
      if (commandStatus == 1) {
        val bError = data[8].toInt() and 0xFF
        throw IOException("CCID command failed (status=0x%02X error=0x%02X)".format(bStatus, bError))
      }
      return data.copyOfRange(10, 10 + dwLength)
    }
  }

  private fun powerOn(): ByteArray {
    // bPowerSelect auto (0x00), then fall back handled by reader.
    writeOut(message(0x62, ByteArray(0), byteArrayOf(0x00, 0x00, 0x00)))
    return readReply()
  }

  private fun powerOff() {
    writeOut(message(0x63, ByteArray(0), byteArrayOf(0x00, 0x00, 0x00)))
    readReply()
  }

  private fun xfrBlock(apdu: ByteArray): ByteArray {
    // bBWI=0, wLevelParameter=0 (short/extended APDU, single block).
    writeOut(message(0x6F, apdu, byteArrayOf(0x00, 0x00, 0x00)))
    return readReply()
  }

  companion object {
    private const val TIMEOUT_MS = 15000
    private const val READ_BUF = 512

    /** First connected device exposing a CCID (class 0x0B) interface, or null. */
    fun findReader(manager: UsbManager): UsbDevice? {
      for (device in manager.deviceList.values) {
        for (i in 0 until device.interfaceCount) {
          if (device.getInterface(i).interfaceClass == UsbConstants.USB_CLASS_CSCID) {
            return device
          }
        }
      }
      return null
    }

    /** Open a CCID connection to [device] (permission must already be granted). */
    fun open(manager: UsbManager, device: UsbDevice): UsbCcidConnection {
      var ccidIface: UsbInterface? = null
      for (i in 0 until device.interfaceCount) {
        val itf = device.getInterface(i)
        if (itf.interfaceClass == UsbConstants.USB_CLASS_CSCID) {
          ccidIface = itf
          break
        }
      }
      val iface = ccidIface ?: throw IOException("No CCID interface on the reader")

      var inEp: UsbEndpoint? = null
      var outEp: UsbEndpoint? = null
      for (e in 0 until iface.endpointCount) {
        val ep = iface.getEndpoint(e)
        if (ep.type == UsbConstants.USB_ENDPOINT_XFER_BULK) {
          if (ep.direction == UsbConstants.USB_DIR_IN) inEp = ep else outEp = ep
        }
      }
      if (inEp == null || outEp == null) throw IOException("CCID reader has no bulk endpoints")

      val conn = manager.openDevice(device) ?: throw IOException("Could not open the USB reader")
      if (!conn.claimInterface(iface, true)) {
        conn.close()
        throw IOException("Could not claim the reader interface")
      }
      return UsbCcidConnection(conn, iface, inEp, outEp)
    }
  }
}
