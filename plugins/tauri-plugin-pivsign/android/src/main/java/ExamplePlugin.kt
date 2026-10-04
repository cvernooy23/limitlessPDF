package com.plugin.pivsign

import android.app.Activity
import android.security.KeyChain
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.bouncycastle.cert.jcajce.JcaCertStore
import org.bouncycastle.cms.CMSProcessableByteArray
import org.bouncycastle.cms.CMSSignedDataGenerator
import org.bouncycastle.cms.jcajce.JcaSignerInfoGeneratorBuilder
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import org.bouncycastle.operator.jcajce.JcaDigestCalculatorProviderBuilder
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.PrivateKey
import java.security.cert.X509Certificate

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

  // List signing identities. For "keychain" this launches the system
  // credential chooser (covers imported soft certs and MDM/Purebred derived
  // PIV credentials) and returns the one the user picked.
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
      else -> invoke.reject("Signing source not supported yet: ${args.source}")
    }
  }

  // Produce a detached PKCS#7 / CMS SignedData over the content file, signing
  // with the chosen credential's private key.
  @Command
  fun signData(invoke: Invoke) {
    val args = invoke.parseArgs(SignDataArgs::class.java)
    when (args.source) {
      "keychain" -> {
        Thread {
          try {
            val key = KeyChain.getPrivateKey(activity, args.id)
            val chain = KeyChain.getCertificateChain(activity, args.id)
            if (key == null || chain == null || chain.isEmpty()) {
              invoke.reject("Could not load the private key / certificate chain")
              return@Thread
            }
            val content = File(args.contentPath).readBytes()
            val pkcs7 = buildDetachedCms(content, key, chain)
            val ret = JSObject()
            ret.put("pkcs7B64", Base64.encodeToString(pkcs7, Base64.NO_WRAP))
            invoke.resolve(ret)
          } catch (e: Exception) {
            invoke.reject("Signing failed: ${e.message}")
          }
        }.start()
      }
      else -> invoke.reject("Signing source not supported yet: ${args.source}")
    }
  }

  private fun buildDetachedCms(
    content: ByteArray,
    key: PrivateKey,
    chain: Array<X509Certificate>
  ): ByteArray {
    val sigAlgo = if (key.algorithm.equals("EC", true)) "SHA256withECDSA" else "SHA256withRSA"
    // No explicit provider: JCA routes signing to the KeyChain/AndroidKeyStore
    // backend that owns the (non-exportable) private key.
    val signer = JcaContentSignerBuilder(sigAlgo).build(key)
    val digestProvider = JcaDigestCalculatorProviderBuilder().build()
    val gen = CMSSignedDataGenerator()
    gen.addSignerInfoGenerator(
      JcaSignerInfoGeneratorBuilder(digestProvider).build(signer, chain[0])
    )
    gen.addCertificates(JcaCertStore(chain.toList()))
    val signed = gen.generate(CMSProcessableByteArray(content), false)
    return signed.encoded
  }
}
