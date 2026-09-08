import org.lwjgl.glfw.GLFW;
import org.lwjgl.input.Mouse;

public final class CursorAgentSmoke {
    private CursorAgentSmoke() {}

    public static void main(String[] arguments) {
        GLFW.glfwSetInputMode(1L, 0x00033001, 0x00034003);
        GLFW.glfwSetInputMode(1L, 0x00033001, 0x00034001);
        Mouse.setGrabbed(true);
        Mouse.setGrabbed(false);
    }
}
