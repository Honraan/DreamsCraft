package io.github.artcraftandroid.procraft.gallery

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ArtworkFileTest {
    @Test
    fun editableProjectKeepsTimelineLayersAndOnlyReferencedMedia() {
        val work = Files.createTempDirectory("dreamscraft-project").toFile()
        try {
            val project = File(work, "project").apply { mkdirs() }
            val archive = File(work, "test.procraft")
            val opened = File(work, "opened")
            val json = """{"document":{"timeline":{"playhead":42}}}"""
            val audio = "audio-1-0000000000000042.pcm"
            val video = "video-00000000000000000000000000000002.media"
            val forgotten = "video-00000000000000000000000000000003.media"
            File(project, "artwork.json").writeText(json)
            File(project, "layers").mkdirs()
            File(project, "layers/4.png").writeBytes(byteArrayOf(4))
            File(project, "media").mkdirs()
            File(project, "media/$audio").writeBytes(byteArrayOf(1, 2))
            File(project, "media/$video").writeBytes(byteArrayOf(3, 4))
            File(project, "media/$forgotten").writeBytes(byteArrayOf(5))

            ArtworkFile.write(project, archive, listOf(audio, video))
            assertTrue(archive.inputStream().use { ArtworkFile.read(it, opened) })
            assertEquals(json, File(opened, "artwork.json").readText())
            assertArrayEquals(byteArrayOf(4), File(opened, "layers/4.png").readBytes())
            assertArrayEquals(byteArrayOf(1, 2), File(opened, "media/$audio").readBytes())
            assertArrayEquals(byteArrayOf(3, 4), File(opened, "media/$video").readBytes())
            assertFalse(File(opened, "media/$forgotten").exists())
        } finally {
            work.deleteRecursively()
        }
    }
}
