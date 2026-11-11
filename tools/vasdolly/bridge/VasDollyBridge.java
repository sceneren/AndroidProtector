import com.android.apksig.ApkVerifier;
import com.tencent.vasdolly.reader.ChannelReader;
import com.tencent.vasdolly.verify.VerifyApk;
import com.tencent.vasdolly.writer.ChannelWriter;
import java.io.File;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;

/** Thin adapter: all channel encoding and signature verification use VasDolly. */
public final class VasDollyBridge {
    public static void main(String[] args) throws Exception {
        if (args.length != 3) {
            throw new IllegalArgumentException("Expected INPUT.apk OUTPUT.apk CHANNEL");
        }
        File input = new File(args[0]);
        File output = new File(args[1]);
        String channel = args[2];
        if (input.getCanonicalFile().equals(output.getCanonicalFile())) {
            throw new IllegalArgumentException("Input and output must be different files");
        }
        ApkVerifier.Result before = VerifyApk.getVerifierResult(input);
        if (!before.isVerified()) {
            throw new IllegalArgumentException("Source APK signature is invalid: " + before.getErrors());
        }
        boolean signingBlock = before.isVerifiedUsingV2Scheme() || before.isVerifiedUsingV3Scheme();
        Files.copy(input.toPath(), output.toPath(), StandardCopyOption.REPLACE_EXISTING);
        if (signingBlock) {
            ChannelWriter.addChannelByV2(output, channel, true);
        } else if (before.isVerifiedUsingV1Scheme()) {
            String previous = ChannelReader.getChannelByV1(output);
            if (previous != null && !previous.isEmpty()) {
                ChannelWriter.removeChannelByV1(output);
            }
            ChannelWriter.addChannelByV1(output, channel);
        } else {
            throw new IllegalArgumentException("APK must have a V1, V2 or V3 signature");
        }
        String actual = signingBlock ? ChannelReader.getChannelByV2(output) : ChannelReader.getChannelByV1(output);
        if (!channel.equals(actual)) {
            throw new IllegalStateException("Channel read-back mismatch");
        }
        ApkVerifier.Result after = VerifyApk.getVerifierResult(output);
        if (!after.isVerified()
                || before.isVerifiedUsingV1Scheme() != after.isVerifiedUsingV1Scheme()
                || before.isVerifiedUsingV2Scheme() != after.isVerifiedUsingV2Scheme()
                || before.isVerifiedUsingV3Scheme() != after.isVerifiedUsingV3Scheme()) {
            throw new IllegalStateException("Output APK signature verification failed: " + after.getErrors());
        }
        // Only emitted once both channel read-back and signature checks pass.
        System.out.println("VASDOLLY_VERIFIED_CHANNEL:" + actual);
    }
}
