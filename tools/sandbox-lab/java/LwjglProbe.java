import org.lwjgl.glfw.*;
import org.lwjgl.opengl.*;
import org.lwjgl.system.MemoryUtil;

/** Brief visible window and framebuffer readback; does not claim input/audio coverage. */
public class LwjglProbe {
    public static void main(String[] args) {
        System.out.println("LWJGL: " + org.lwjgl.Version.getVersion());
        GLFWErrorCallback.createPrint(System.err).set();
        if (!GLFW.glfwInit()) throw new IllegalStateException("glfwInit failed");
        long window = 0;
        try {
            GLFW.glfwWindowHint(GLFW.GLFW_CONTEXT_VERSION_MAJOR, 3);
            GLFW.glfwWindowHint(GLFW.GLFW_CONTEXT_VERSION_MINOR, 2);
            GLFW.glfwWindowHint(GLFW.GLFW_OPENGL_PROFILE, GLFW.GLFW_OPENGL_CORE_PROFILE);
            GLFW.glfwWindowHint(GLFW.GLFW_OPENGL_FORWARD_COMPAT, GLFW.GLFW_TRUE);
            window = GLFW.glfwCreateWindow(480, 300, "MonaLauncher sandbox probe", 0, 0);
            if (window == 0) throw new IllegalStateException("glfwCreateWindow failed");
            System.out.println("CHECK\tglfw_window\ttrue");
            GLFW.glfwMakeContextCurrent(window);
            GL.createCapabilities();
            System.out.println("OpenGL renderer: " + GL11.glGetString(GL11.GL_RENDERER));
            GL11.glClearColor(0.25f, 0.5f, 0.75f, 1.0f);
            GL11.glClear(GL11.GL_COLOR_BUFFER_BIT);
            var pixel = MemoryUtil.memAlloc(4);
            try {
                GL11.glReadPixels(0, 0, 1, 1, GL11.GL_RGBA, GL11.GL_UNSIGNED_BYTE, pixel);
                int error = GL11.glGetError();
                System.out.printf("Framebuffer: rgba=%d,%d,%d,%d glError=%d%n",
                    Byte.toUnsignedInt(pixel.get(0)), Byte.toUnsignedInt(pixel.get(1)),
                    Byte.toUnsignedInt(pixel.get(2)), Byte.toUnsignedInt(pixel.get(3)), error);
                if (error != GL11.GL_NO_ERROR
                    || Math.abs(Byte.toUnsignedInt(pixel.get(0)) - 64) > 2
                    || Math.abs(Byte.toUnsignedInt(pixel.get(1)) - 128) > 2
                    || Math.abs(Byte.toUnsignedInt(pixel.get(2)) - 191) > 2)
                    throw new IllegalStateException("unexpected framebuffer pixel");
                System.out.println("CHECK\topengl_readback\ttrue");
            } finally { MemoryUtil.memFree(pixel); }
            GLFW.glfwSwapBuffers(window);
            double until = GLFW.glfwGetTime() + 1.5;
            while (GLFW.glfwGetTime() < until && !GLFW.glfwWindowShouldClose(window)) GLFW.glfwWaitEventsTimeout(0.05);
        } finally {
            if (window != 0) GLFW.glfwDestroyWindow(window);
            GLFW.glfwTerminate();
            var callback = GLFW.glfwSetErrorCallback(null);
            if (callback != null) callback.free();
        }
    }
}
