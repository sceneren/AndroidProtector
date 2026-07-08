package com.androidprotector.channel

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * 渠道读取工具类。
 *
 * 这个类用于业务 Android 项目读取“Android 加固工具”写入的渠道信息。
 *
 * 写入位置：
 * - APK Signing Block
 * - 自定义 ID：0x71777777
 * - 当前 payload 格式：{"chn":"huawei"}
 *
 * 使用方式：
 * ```
 * val channel = ChannelReader.getChannel(context, "default")
 * ```
 *
 * 注意：
 * - 只支持 APK，不支持 AAB 运行时直接读取。
 * - 读取失败、没有渠道信息、APK 结构不符合预期时，会返回 defaultValue 或 null。
 * - 该类不依赖加固工具运行时，可直接复制到业务 App 工程中使用。
 */
object ChannelReader {
    /** 加固工具写入渠道信息时使用的 APK Signing Block 自定义 ID。 */
    private const val APK_CHANNEL_BLOCK_ID = 0x71777777

    /** ZIP End of Central Directory 的最小长度。 */
    private const val EOCD_MIN_SIZE = 22

    /** ZIP comment 最大长度，EOCD 最多向前搜索 65535 字节。 */
    private const val EOCD_MAX_COMMENT_LENGTH = 0xffff

    /** ZIP EOCD 标识，小端读取后应为 0x06054b50。 */
    private const val ZIP_EOCD_REC_SIG = 0x06054b50

    /** ZIP64 情况下 central directory offset 的占位值，本工具类当前不处理 ZIP64 APK。 */
    private const val ZIP64_CENTRAL_DIR_OFFSET = 0xffffffffL

    /** APK Signing Block magic 的前 8 字节，小端 long。 */
    private const val APK_SIG_BLOCK_MAGIC_FIRST = 0x20676953204b5041L

    /** APK Signing Block magic 的后 8 字节，小端 long。 */
    private const val APK_SIG_BLOCK_MAGIC_SECOND = 0x3234206b636f6c42L

    /**
     * 读取当前 App 的渠道名。
     *
     * @param context 任意 Context，通常传 Application 或 Activity。
     * @param defaultValue 当没有渠道信息或读取失败时返回的默认值。
     * @return 渠道名，例如 huawei、xiaomi、oppo、vivo、honor、yyb；失败时返回 defaultValue。
     */
    @JvmStatic
    fun getChannel(context: Context, defaultValue: String? = null): String? {
        val payload = readChannelPayload(context).orEmpty()
        if (payload.isEmpty()) return defaultValue

        // 当前写入格式是 JSON：{"chn":"huawei"}。
        // 如果未来写入的是纯字符串，这里会 fallback 返回原始 payload。
        return runCatching {
            JSONObject(payload).optString("chn").takeIf { it.isNotEmpty() }
        }.getOrNull() ?: payload
    }

    /**
     * 读取当前 App APK 中的原始渠道 payload。
     *
     * @param context 任意 Context。
     * @return APK Signing Block 中写入的原始字符串，例如 {"chn":"huawei"}；失败时返回 null。
     */
    @JvmStatic
    fun readChannelPayload(context: Context): String? {
        val sourceDir = context.applicationInfo?.sourceDir ?: return null
        return readChannelPayload(File(sourceDir))
    }

    /**
     * 从指定 APK 文件中读取原始渠道 payload。
     *
     * 这个方法方便在测试代码或工具代码里直接传入 APK 文件。
     *
     * @param apkFile 已签名且写入渠道信息的 APK 文件。
     * @return APK Signing Block 中写入的原始字符串；失败时返回 null。
     */
    @JvmStatic
    fun readChannelPayload(apkFile: File): String? {
        if (!apkFile.isFile) return null
        return try {
            RandomAccessFile(apkFile, "r").use { file ->
                val centralDirOffset = findCentralDirectoryOffset(file)
                if (centralDirOffset < 32) return null

                // APK Signing Block 位于 Central Directory 前面。
                readSigningBlockValue(file, centralDirOffset, APK_CHANNEL_BLOCK_ID)
            }
        } catch (_: IOException) {
            null
        }
    }

    /**
     * 查找 ZIP Central Directory 的起始偏移。
     *
     * APK 本质是 ZIP。需要先找到 EOCD，再从 EOCD 中读取 Central Directory Offset。
     * APK Signing Block 位于 Central Directory 之前。
     */
    @Throws(IOException::class)
    private fun findCentralDirectoryOffset(file: RandomAccessFile): Long {
        val fileLength = file.length()
        if (fileLength < EOCD_MIN_SIZE) return -1

        val maxSearch = minOf(fileLength - EOCD_MIN_SIZE, EOCD_MAX_COMMENT_LENGTH.toLong())
        for (commentLength in 0..maxSearch) {
            val eocdOffset = fileLength - EOCD_MIN_SIZE - commentLength
            file.seek(eocdOffset)
            if (readLittleEndianInt(file) != ZIP_EOCD_REC_SIG) continue

            // EOCD 最后两个字节是 comment length，用它确认当前位置是真正 EOCD。
            file.seek(eocdOffset + 20)
            val actualCommentLength = readLittleEndianShort(file)
            if (actualCommentLength.toLong() != commentLength) continue

            // EOCD + 16 是 Central Directory Offset。
            file.seek(eocdOffset + 16)
            val centralDirOffset = readLittleEndianInt(file).toLong() and 0xffffffffL
            return if (centralDirOffset == ZIP64_CENTRAL_DIR_OFFSET) -1 else centralDirOffset
        }
        return -1
    }

    /**
     * 从 APK Signing Block 中读取指定 ID 对应的 value。
     *
     * APK Signing Block 结构：
     * - size in bytes
     * - 多组 ID-value pair
     * - size in bytes
     * - magic
     *
     * 每组 pair 结构：
     * - uint64 pair size
     * - uint32 id
     * - bytes value
     */
    @Throws(IOException::class)
    private fun readSigningBlockValue(
        file: RandomAccessFile,
        centralDirOffset: Long,
        targetId: Int
    ): String? {
        // footer 长度固定 24 字节：8 字节 size + 16 字节 magic。
        file.seek(centralDirOffset - 24)
        val blockSizeInFooter = readLittleEndianLong(file)
        val magicFirst = readLittleEndianLong(file)
        val magicSecond = readLittleEndianLong(file)
        if (magicFirst != APK_SIG_BLOCK_MAGIC_FIRST || magicSecond != APK_SIG_BLOCK_MAGIC_SECOND) {
            return null
        }

        val totalBlockSize = blockSizeInFooter + 8
        val blockOffset = centralDirOffset - totalBlockSize
        if (blockOffset < 0 || totalBlockSize < 32) return null

        // header 和 footer 中的 size 必须一致。
        file.seek(blockOffset)
        val blockSizeInHeader = readLittleEndianLong(file)
        if (blockSizeInHeader != blockSizeInFooter) return null

        val pairsEnd = centralDirOffset - 24
        var cursor = blockOffset + 8
        while (cursor < pairsEnd) {
            file.seek(cursor)
            val pairSize = readLittleEndianLong(file)
            if (pairSize < 4 || pairSize > Int.MAX_VALUE.toLong() || cursor + 8 + pairSize > pairsEnd) {
                return null
            }

            val id = readLittleEndianInt(file)
            val valueSize = pairSize.toInt() - 4
            if (id == targetId) {
                val value = ByteArray(valueSize)
                file.readFully(value)
                return String(value, Charsets.UTF_8)
            }

            cursor += 8 + pairSize
        }
        return null
    }

    /** 读取 2 字节小端无符号整数。 */
    @Throws(IOException::class)
    private fun readLittleEndianShort(file: RandomAccessFile): Int {
        val bytes = ByteArray(2)
        file.readFully(bytes)
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).short.toInt() and 0xffff
    }

    /** 读取 4 字节小端整数。 */
    @Throws(IOException::class)
    private fun readLittleEndianInt(file: RandomAccessFile): Int {
        val bytes = ByteArray(4)
        file.readFully(bytes)
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).int
    }

    /** 读取 8 字节小端整数。 */
    @Throws(IOException::class)
    private fun readLittleEndianLong(file: RandomAccessFile): Long {
        val bytes = ByteArray(8)
        file.readFully(bytes)
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).long
    }
}
