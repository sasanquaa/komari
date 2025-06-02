import kmNet
import grpc
import pyautogui
import time

from random import Random
from concurrent import futures
# The two imports below is generated from:
# python -m grpc_tools.protoc --python_out=. --pyi_out=. --grpc_python_out=. -I../../backend/proto ../..
# /backend/proto/input.proto
from input_pb2 import Key, KeyRequest, KeyResponse, KeyDownRequest, KeyDownResponse, KeyUpRequest, KeyUpResponse, KeyInitRequest, KeyInitResponse, MouseRequest, MouseResponse, MouseAction, Coordinate
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, keys_map: dict[Key, int]) -> None:
        super().__init__()
        self.keys_map = keys_map
        self.seed = None

    # This is the init function that is called each time the bot connects to your service.
    def Init(self, request: KeyInitRequest, context):
        # This is a seed generated automatically by the bot for the first time the bot is run.
        # The seed is saved in the database and reused again later.
        # If you do not wish to use the bot provided delay for key down press, you can use this
        # seed for generating delay timing. The seed is a 32 bytes array.
        self.seed = request.seed

        # There are two types of mouse coordinate depending on your setup:
        # - Relative: The MouseRequest coordinates (x, y, width, height) is relative to the
        #   current window the bot is capturing. For example, if you play your game in 1366x768,
        #   then (width, height) = (1366, 768) and (x, y) is offset from top-left corner of that
        #   window with (0, 0) being top-left and (width, height) is bottom-right.
        #
        # - Screen: The MouseRequest coordinates (x, y, width, height) is relative to the
        #   current monitor screen of the app the bot is capturing (which monitor the app is in).
        #   With (0, 0) being top-left of that monitor screen and (width, height) is bottom-right.
        #   For example, your game might be (1366, 768) but it is running in the monitor of size
        #   (1920, 1080) so (width, height) = (1920, 1080).
        #
        # You should return the one appropriate for your setup in this Init() function.
        # return KeyInitResponse(mouse_coordinate=Coordinate.Screen)
        return KeyInitResponse(mouse_coordinate=Coordinate.Relative)

    def SendMouse(self, request: MouseRequest, context):
        # Regardless of the type of Coordinate return in Init(), the coordinates are always based on
        # the PC the bot is running in. And there are two cases you should consider:
        #
        # - If you run this server on a separate PC than the bot PC and use remote control, this
        #   coordinate is NOT local to the server PC
        #
        # - If you run this server on the same PC as the bot, this coordinate is local to
        #   the server PC
        #
        # The coordinates x, y represent the location the bot wants the input server to click
        # relative to the PC the bot is in. Therefore, it must be transformed first to match your
        # current setup and also to the x, y values your input method can use.
        #
        # For example, KMBox requires x, y values to be relative while SendInput requires
        # the x, y values to be absolute in the range [0, 65535].
        width = request.width
        height = request.height
        x = request.x
        y = request.y
        action = request.action

        # KMBox mouse move is relative the current cursor position.
        #
        # Case 1: KMBox input server is in the same PC as bot. This case is easier to handle,
        # all you need the current cursor position in order to get the relative movement amount.
        #
        # You need to use Coordinate.Screen for this case.
        # position = pyautogui.position()
        # dx = x - position.x
        # dy = y - position.y

        # Case 2: KMBox input server is in a different PC than the bot. This case can be
        # problematic depending on your setup. For instance, if you use GF Now, when running
        # the game, there are no "status bars" or other non-game UI areas. Your game will always show
        # without any kind of border/bars that might inset the actual game. But if you run your
        # game in something like a VM or Sunshine/Moonlight, these apps can have these
        # bars and the bot always capture the full app and not just the game being shown. So
        # you need to subtract the coordinates by some amount until it feels "correct".
        #
        # You need to use Coordinate.Relative for this case.

        # These are for cropping the non-game UI portion of the app the game is running in.
        # For Moonlight/Sunshine, you can leave this as-is. This method can be unreliable due
        # this reason. You can also use PowerToys Screen Ruler to measure this non-game UI area.

        # Make sure you turn off 'Enhance pointer precision' in 'Mouse Properties' settings. That
        # seems to mess with KMBox relative movement. Pointer speed also affects the movement so
        # you should change it to the default speed (6).
        screen_width, screen_height = pyautogui.size()
        position = pyautogui.position()

        # Map coordinates from bot PC to input PC
        crop_left_px = 0  # Change this until it feels correct
        crop_top_px = 30  # Change this until it feels correct
        scaled_x = int(
            ((x - crop_left_px) / (width - crop_left_px)) * screen_width)
        scaled_y = int(
            ((y - crop_top_px) / (height - crop_top_px)) * screen_height)

        dx = scaled_x - position.x
        dy = scaled_y - position.y

        # Common logics, not very human but just an example
        seed_int = int.from_bytes(self.seed[:4], "little", signed=False)
        rnd = Random(seed_int)
        ms = rnd.randrange(200, 300)
        if action == MouseAction.Move:
            kmNet.move_auto(dx, dy, ms)
        elif action == MouseAction.Click:
            kmNet.move_auto(dx, dy, ms)
            kmNet.mouse(1, 0, 0, 0)
            kmNet.mouse(0, 0, 0, 0)
        elif action == MouseAction.ScrollDown:
            kmNet.move_auto(dx, dy, ms)
            kmNet.mouse(0, 0, 0, -1)

        # Sleep to ensure mouse movement completes since KMBox move_auto doesn't seem to block until
        # the move is actually complete.
        # If you let the mouse jump instead of sliding like above, sleep is probably not needed.
        time.sleep(ms / 1000)

        return MouseResponse()

    def Send(self, request: KeyRequest, context):
        # This `key` is an enum representing the key the bot want your customized input to send.
        # You should map this to the key supported by your customized input method.
        key = self.keys_map[request.key]
        # This is key down sleep milliseconds. It is generated automatically by the bot using the
        # above seed. You should use this delay and `time.sleep(delay)` on key down.
        key_down = request.down_ms / 1000.0

        kmNet.keydown(key)
        time.sleep(key_down)
        kmNet.keyup(key)
        return KeyResponse()

    def SendUp(self, request: KeyUpRequest, context):
        kmNet.keyup(self.keys_map[request.key])
        return KeyUpResponse()

    def SendDown(self, request: KeyDownRequest, context):
        kmNet.keydown(self.keys_map[request.key])
        return KeyDownResponse()


if __name__ == "__main__":
    kmNet.init("192.168.2.188", "8704", "33005C53")
    # Generated with ChatGPT, might not be accurate
    keys_map = {
        # Letters A-Z
        # A=0 -> HID 4, ..., Z=25 -> HID 29
        **{Key.Value(Key.Name(i)): 4 + i for i in range(26)},

        # Digits 0–9
        Key.Zero: 39,
        Key.One: 30,
        Key.Two: 31,
        Key.Three: 32,
        Key.Four: 33,
        Key.Five: 34,
        Key.Six: 35,
        Key.Seven: 36,
        Key.Eight: 37,
        Key.Nine: 38,

        # Function keys
        Key.F1: 58,
        Key.F2: 59,
        Key.F3: 60,
        Key.F4: 61,
        Key.F5: 62,
        Key.F6: 63,
        Key.F7: 64,
        Key.F8: 65,
        Key.F9: 66,
        Key.F10: 67,
        Key.F11: 68,
        Key.F12: 69,

        # Arrows & navigation
        Key.Up: 82,
        Key.Down: 81,
        Key.Left: 80,
        Key.Right: 79,
        Key.Home: 74,
        Key.End: 77,
        Key.PageUp: 75,
        Key.PageDown: 78,
        Key.Insert: 73,
        Key.Delete: 76,

        # Modifiers and special characters
        Key.Ctrl: 224,
        Key.Enter: 40,
        Key.Space: 44,
        Key.Tilde: 53,
        Key.Quote: 52,
        Key.Semicolon: 51,
        Key.Comma: 54,
        Key.Period: 55,
        Key.Slash: 56,
        Key.Esc: 41,
        Key.Shift: 225,
        Key.Alt: 226,
    }

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    add_KeyInputServicer_to_server(KeyInput(keys_map), server)
    server.add_insecure_port("[::]:5001")
    server.start()
    print("Server started, listening on 5001")
    server.wait_for_termination()
