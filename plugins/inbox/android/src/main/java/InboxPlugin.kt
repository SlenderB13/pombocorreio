package dev.pombocorreio.inbox

import android.app.Activity
import android.content.ContentValues
import android.os.Build
import android.provider.MediaStore
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileInputStream

@InvokeArg
class PublishArgs {
    lateinit var sourcePath: String
    lateinit var displayName: String
    lateinit var mimeType: String
}

@TauriPlugin
class InboxPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun publish(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            invoke.reject("Public Downloads requires Android 10 or newer")
            return
        }

        val args = invoke.parseArgs(PublishArgs::class.java)
        val source = File(args.sourcePath)
        val resolver = activity.contentResolver
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, args.displayName)
            put(MediaStore.Downloads.MIME_TYPE, args.mimeType)
            put(MediaStore.Downloads.RELATIVE_PATH, "Download/Pombo Correio")
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        if (uri == null) {
            invoke.reject("Android could not create the download")
            return
        }

        try {
            resolver.openOutputStream(uri, "w").use { output ->
                requireNotNull(output) { "Android could not open the download" }
                FileInputStream(source).use { input -> input.copyTo(output) }
            }
            values.clear()
            values.put(MediaStore.Downloads.IS_PENDING, 0)
            resolver.update(uri, values, null, null)
            invoke.resolve(JSObject().apply { put("uri", uri.toString()) })
        } catch (error: Exception) {
            resolver.delete(uri, null, null)
            invoke.reject(error.message ?: "Could not publish the download")
        }
    }
}

