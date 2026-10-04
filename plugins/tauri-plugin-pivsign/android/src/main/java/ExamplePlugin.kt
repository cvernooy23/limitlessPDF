package com.plugin.pivsign

import android.app.Activity
import android.app.AlertDialog
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Build
import android.security.KeyChain
import android.text.InputType
import android.util.Base64
import android.widget.EditText
import android.widget.Toast
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.yubico.yubikit.android.YubiKitManager
import com.yubico.yubikit.android.transport.nfc.NfcConfiguration
import com.yubico.yubikit.android.transport.nfc.NfcNotAvailable
import com.yubico.yubikit.android.transport.nfc.NfcYubiKeyDevice
import com.yubico.yubikit.core.smartcard.SmartCardConnection
import com.yubico.yubikit.piv.KeyType
import com.yubico.yubikit.piv.PivSession
import com.yubico.yubikit.piv.Slot
import org.bouncycastle.asn1.x509.AlgorithmIdentifier
import org.bouncycastle.cert.jcajce.JcaCertStore
import org.bouncycastle.cert.jcajce.JcaX509CertificateHolder
import org.bouncycastle.cms.CMSProcessableByteArray
import org.bouncycastle.cms.CMSSignedDataGenerator
import org.bouncycastle.cms.jcajce.JcaSignerInfoGeneratorBuilder
import org.bouncycastle.operator.ContentSigner
import org.bouncycastle.operator.DefaultSignatureAlgorithmIdentifierFinder
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import org.bouncycastle.operator.jcajce.JcaDigestCalculatorProviderBuilder
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.OutputStream
import java.security.PrivateKey
import java.security.Signature
import java.security.cert.X509Certificate
import java.util.concurrent.atomic.AtomicBoolean

private const val ACTION_USB_PERMISSION = "com.plugin.pivsign.USB_PERMISSION"

@InvokeArg
class ListIdentitiesArgs {
  var source: String = "keychain"
}

@InvokeArg
class SignDataArgs {
  var source: String = "keychain"
  var id: String = ""
  var contentPath: String = ""
}

@TauriPlugin
class ExamplePlugin(private val activity: Activity) : Plugin(activity) {

  // ── Identity listing ──────────────────────────────────────────────────
  // For "keychain" this launches the system credential chooser (covers
  // imported soft certs and MDM/Purebred derived PIV credentials) and returns
  // the picked identity. Hardware tokens (nfc) are read at sign time.
  @Command
  fun listIdentities(invoke: Invoke) {
    val args = invoke.parseArgs(ListIdentitiesArgs::class.java)
    when (args.source) {
      "keychain" -> {
        KeyChain.choosePrivateKeyAlias(
          activity,
          { alias ->
            if (alias == null) {
              invoke.reject("No credential selected")
            } else {
              Thread {
                try {
                  val chain = KeyChain.getCertificateChain(activity, alias)
                  if (chain == null || chain.isEmpty()) {
                    invoke.reject("No certificate found for the selected credential")
                  } else {
                    val leaf = chain[0]
                    val entry = JSONObject()
                    entry.put("id", alias)
                    entry.put("subject", leaf.subjectDN.name)
                    entry.put("issuer", leaf.issuerDN.name)
                    val arr = JSONArray()
                    arr.put(entry)
                    val ret = JSObject()
                    ret.put("identities", arr)
                    invoke.resolve(ret)
                  }
                } catch (e: Exception) {
                  invoke.reject("KeyChain error: ${e.message}")
                }
              }.start()
            }
          },
          null,
          null,
          null,
          -1,
          null
        )
      }
      else -> invoke.reject("Listing is not supported for source: ${args.source}")
    }
  }

  // ── Signing ───────────────────────────────────────────────────────────
  @Command
  fun signData(invoke: Invoke) {
    val args = invoke.parseArgs(SignDataArgs::class.java)
    when (args.source) {
      "keychain" -> signWithKeyChain(invoke, args)
      "nfc" -> signWithNfc(invoke, args)
      "usb" -> signWithUsb(invoke, args)
      else -> invoke.reject("Signing source not supported yet: ${args.source}")
    }
  }

  private fun signWithKeyChain(invoke: Invoke, args: SignDataArgs) {
    Thread {
      try {
        val key = KeyChain.getPrivateKey(activity, args.id)
        val chain = KeyChain.getCertificateChain(activity, args.id)
        if (key == null || chain == null || chain.isEmpty()) {
          invoke.reject("Could not load the private key / certificate chain")
          return@Thread
        }
        val content = File(args.contentPath).readBytes()
        val pkcs7 = buildCmsWithJcaKey(content, key, chain)
        resolvePkcs7(invoke, pkcs7)
      } catch (e: Exception) {
        invoke.reject("Signing failed: ${e.message}")
      }
    }.start()
  }

  // NFC PIV: prompt for the PIN, then sign over a single tap using YubiKit's
  // PIV protocol (YubiKey 5 NFC and NFC-capable PIV tokens). The card's key is
  // non-exportable; it produces the raw signature, we assemble the CMS here.
  private fun signWithNfc(invoke: Invoke, args: SignDataArgs) {
    val content: ByteArray = try {
      File(args.contentPath).readBytes()
    } catch (e: Exception) {
      invoke.reject("Could not read content to sign: ${e.message}")
      return
    }
    promptPin(
      "Enter your PIV PIN, then hold your security key to the back of the phone.",
      onPin = { pin -> startNfcSign(invoke, content, pin) },
      onCancel = { invoke.reject("Cancelled") },
    )
  }

  // Physical PIV card (e.g. a CAC) in a USB-C CCID reader.
  private fun signWithUsb(invoke: Invoke, args: SignDataArgs) {
    val content: ByteArray = try {
      File(args.contentPath).readBytes()
    } catch (e: Exception) {
      invoke.reject("Could not read content to sign: ${e.message}")
      return
    }
    val usbManager = activity.getSystemService(Context.USB_SERVICE) as UsbManager
    val device = UsbCcidConnection.findReader(usbManager)
    if (device == null) {
      invoke.reject("No USB smart-card reader found. Connect a CCID reader with your card inserted.")
      return
    }
    if (usbManager.hasPermission(device)) {
      promptPinThenSignUsb(invoke, content, usbManager, device)
    } else {
      requestUsbPermission(usbManager, device) { granted ->
        if (granted) promptPinThenSignUsb(invoke, content, usbManager, device)
        else invoke.reject("USB permission was denied")
      }
    }
  }

  private fun promptPinThenSignUsb(
    invoke: Invoke,
    content: ByteArray,
    usbManager: UsbManager,
    device: UsbDevice,
  ) {
    promptPin(
      "Enter your PIV PIN to sign with the inserted smart card.",
      onPin = { pin ->
        Thread {
          var conn: UsbCcidConnection? = null
          try {
            conn = UsbCcidConnection.open(usbManager, device)
            val piv = PivSession(conn)
            piv.verifyPin(pin.toCharArray())
            val cert = piv.getCertificate(Slot.SIGNATURE)
            val pkcs7 = buildCmsWithPivCard(content, piv, cert)
            resolvePkcs7(invoke, pkcs7)
          } catch (e: Exception) {
            invoke.reject("USB card signing failed: ${e.message}")
          } finally {
            try {
              conn?.close()
            } catch (_: Exception) {
            }
          }
        }.start()
      },
      onCancel = { invoke.reject("Cancelled") },
    )
  }

  private fun requestUsbPermission(
    usbManager: UsbManager,
    device: UsbDevice,
    callback: (Boolean) -> Unit,
  ) {
    val receiver = object : BroadcastReceiver() {
      override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action == ACTION_USB_PERMISSION) {
          try {
            activity.unregisterReceiver(this)
          } catch (_: Exception) {
          }
          callback(intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false))
        }
      }
    }
    val filter = IntentFilter(ACTION_USB_PERMISSION)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      activity.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
      @Suppress("UnspecifiedRegisterReceiverFlag")
      activity.registerReceiver(receiver, filter)
    }
    val flags =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
    val intent = Intent(ACTION_USB_PERMISSION).setPackage(activity.packageName)
    val pending = PendingIntent.getBroadcast(activity, 0, intent, flags)
    usbManager.requestPermission(device, pending)
  }

  private fun promptPin(message: String, onPin: (String) -> Unit, onCancel: () -> Unit) {
    activity.runOnUiThread {
      val input = EditText(activity).apply {
        inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD
        hint = "PIV PIN"
      }
      AlertDialog.Builder(activity)
        .setTitle("Sign with smart card")
        .setMessage(message)
        .setView(input)
        .setCancelable(false)
        .setPositiveButton("Continue") { _, _ ->
          val pin = input.text.toString()
          if (pin.isEmpty()) onCancel() else onPin(pin)
        }
        .setNegativeButton("Cancel") { _, _ -> onCancel() }
        .show()
    }
  }

  private fun startNfcSign(invoke: Invoke, content: ByteArray, pin: String) {
    val manager = YubiKitManager(activity)
    val done = AtomicBoolean(false)
    try {
      manager.startNfcDiscovery(NfcConfiguration(), activity) { device: NfcYubiKeyDevice ->
        device.requestConnection(SmartCardConnection::class.java) { result ->
          if (done.getAndSet(true)) return@requestConnection
          try {
            val connection = result.value
            val piv = PivSession(connection)
            piv.verifyPin(pin.toCharArray())
            val cert = piv.getCertificate(Slot.SIGNATURE)
            val pkcs7 = buildCmsWithPivCard(content, piv, cert)
            resolvePkcs7(invoke, pkcs7)
          } catch (e: Exception) {
            invoke.reject("NFC signing failed: ${e.message}")
          } finally {
            try {
              manager.stopNfcDiscovery(activity)
            } catch (_: Exception) {
            }
          }
        }
      }
      activity.runOnUiThread {
        Toast.makeText(activity, "Hold your security key to the phone", Toast.LENGTH_LONG).show()
      }
    } catch (e: NfcNotAvailable) {
      invoke.reject("NFC is not available or is turned off: ${e.message}")
    } catch (e: Exception) {
      invoke.reject("Could not start NFC: ${e.message}")
    }
  }

  // ── CMS assembly ──────────────────────────────────────────────────────

  // KeyChain / soft key: BouncyCastle drives the signature with the JCA key
  // (routes to the KeyChain/AndroidKeyStore backend that owns the key).
  private fun buildCmsWithJcaKey(
    content: ByteArray,
    key: PrivateKey,
    chain: Array<X509Certificate>
  ): ByteArray {
    val sigAlgo = if (key.algorithm.equals("EC", true)) "SHA256withECDSA" else "SHA256withRSA"
    val signer = JcaContentSignerBuilder(sigAlgo).build(key)
    val gen = CMSSignedDataGenerator()
    gen.addSignerInfoGenerator(
      JcaSignerInfoGeneratorBuilder(JcaDigestCalculatorProviderBuilder().build())
        .build(signer, chain[0])
    )
    gen.addCertificates(JcaCertStore(chain.toList()))
    return gen.generate(CMSProcessableByteArray(content), false).encoded
  }

  // Hardware PIV token: the card produces the raw signature over the CMS
  // SignedAttributes; we wrap it in a ContentSigner so BouncyCastle assembles
  // a standard detached SignedData.
  private fun buildCmsWithPivCard(
    content: ByteArray,
    piv: PivSession,
    cert: X509Certificate
  ): ByteArray {
    val isRsa = cert.publicKey.algorithm.equals("RSA", true)
    val sigAlgName = if (isRsa) "SHA256withRSA" else "SHA256withECDSA"
    val sigAlgId: AlgorithmIdentifier =
      DefaultSignatureAlgorithmIdentifierFinder().find(sigAlgName)
    val keyType = KeyType.fromKey(cert.publicKey)

    val contentSigner = object : ContentSigner {
      private val buffer = ByteArrayOutputStream()
      override fun getAlgorithmIdentifier(): AlgorithmIdentifier = sigAlgId
      override fun getOutputStream(): OutputStream = buffer
      override fun getSignature(): ByteArray {
        return try {
          @Suppress("DEPRECATION")
          piv.sign(Slot.SIGNATURE, keyType, buffer.toByteArray(), Signature.getInstance(sigAlgName))
        } catch (e: Exception) {
          throw RuntimeException("PIV card signing failed: ${e.message}", e)
        }
      }
    }

    val gen = CMSSignedDataGenerator()
    gen.addSignerInfoGenerator(
      JcaSignerInfoGeneratorBuilder(JcaDigestCalculatorProviderBuilder().build())
        .build(contentSigner, JcaX509CertificateHolder(cert))
    )
    gen.addCertificates(JcaCertStore(listOf(cert)))
    return gen.generate(CMSProcessableByteArray(content), false).encoded
  }

  private fun resolvePkcs7(invoke: Invoke, pkcs7: ByteArray) {
    val ret = JSObject()
    ret.put("pkcs7B64", Base64.encodeToString(pkcs7, Base64.NO_WRAP))
    invoke.resolve(ret)
  }
}
