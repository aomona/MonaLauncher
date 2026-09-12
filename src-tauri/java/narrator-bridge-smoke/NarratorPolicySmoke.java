import com.mojang.text2speech.Narrator;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;

public final class NarratorPolicySmoke {
    public static void main(String[] args) throws Exception {
        boolean allowed = Boolean.parseBoolean(args[0]);
        System.setProperty("monalauncher.narrator.enabled", args[0]);
        System.setProperty("monalauncher.narrator.token", "test-only-token");
        PrintStream original = System.out;
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        System.setOut(new PrintStream(bytes, true, "UTF-8"));
        Narrator narrator = Narrator.getNarrator();
        if (narrator.active() != allowed) throw new AssertionError("wrong active state");
        narrator.say("policy probe", true, 0.5f);
        narrator.clear();
        narrator.destroy();
        System.setOut(original);
        String output = bytes.toString("UTF-8");
        if (allowed ? !output.contains("\tSAY\t1\t0.5\t") : !output.isEmpty()) {
            throw new AssertionError("narrator permission was not enforced");
        }
    }
}
