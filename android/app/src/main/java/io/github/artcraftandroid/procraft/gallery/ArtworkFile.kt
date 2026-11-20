package io.github.artcraftandroid.procraft.gallery

import java.io.File
import java.io.InputStream
import java.util.zip.ZipInputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** Portable editable artwork, with the document, layer pixels and media kept in a single archive. */
object ArtworkFile {
    private val mediaName = Regex("media/(audio-[0-9]+-[0-9a-f]{16}\\.pcm|video-[0-9a-fA-F]{32}\\.media)")

    private fun allowed(name: String): Boolean =
        name == "artwork.json" || name == "thumbnail.png" || name == "plugin-data.bin" ||
            name.matches(Regex("reference(?:-[0-9]+)?\\.png")) ||
            (name.startsWith("layers/") && name.endsWithAny(".png", ".store", ".layerdata", ".vector")) ||
            name.startsWith("timelapse/") || name == "about/photo.jpg" || name == "about/signature.json" ||
            mediaName.matches(name)

    private fun String.endsWithAny(vararg suffixes: String) = suffixes.any { endsWith(it) }

    /** Copies the saved artwork into a portable file, streaming one entry at a time. */
    fun write(dir: File, out: File, media: List<String>) {
        val root = dir.canonicalFile
        val mediaPaths = media.map { "media/$it" }.toSet()
        check(File(root, "artwork.json").isFile && mediaPaths.all { mediaName.matches(it) && File(root, it).isFile }) {
            "The project or one of its media files is missing"
        }
        val tmp = File.createTempFile("dreamscraft-", ".zip", out.parentFile)
        try {
            ZipOutputStream(tmp.outputStream().buffered()).use { zip ->
                for (file in root.walkTopDown().filter { it.isFile }) {
                    if (file.canonicalFile != file.absoluteFile || !file.canonicalPath.startsWith(root.path + File.separator)) continue
                    val name = file.relativeTo(root).invariantSeparatorsPath
                    if (!allowed(name) || name.contains("..") || (name.startsWith("media/") && name !in mediaPaths)) continue
                    zip.putNextEntry(ZipEntry(name))
                    file.inputStream().use { it.copyTo(zip) }
                    zip.closeEntry()
                }
            }
            check(tmp.renameTo(out)) { "Could not finish the editable project" }
        } finally {
            tmp.delete()
        }
    }

    /**
     * Unzips an artwork file into the empty folder [dir]; false (and nothing kept) if it isn't
     * one. Only the files an artwork has are taken, and none outside the folder.
     */
    fun read(input: InputStream, dir: File): Boolean {
        val root = dir.canonicalFile
        var found = false
        ZipInputStream(input.buffered()).use { zip ->
            while (true) {
                val entry = zip.nextEntry ?: break
                val name = entry.name
                if (entry.isDirectory || !allowed(name) || name.contains("..") || '\\' in name) continue
                val target = File(root, name).canonicalFile
                if (!target.path.startsWith(root.path + File.separator)) continue
                target.parentFile?.mkdirs()
                target.outputStream().use { zip.copyTo(it) }
                if (name == "artwork.json") found = true
            }
        }
        if (!found) dir.deleteRecursively()
        return found
    }
}
