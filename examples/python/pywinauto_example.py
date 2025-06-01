import pywinauto
import pyautogui
import grpc
import time

from concurrent import futures
from pywinauto import WindowSpecification, keyboard
from pywinauto.application import Application
# The two imports below is generated from:
# python -m grpc_tools.protoc --python_out=. --pyi_out=. --grpc_python_out=. -I../../backend/proto ../..
# /backend/proto/input.proto
from input_pb2 import Key, KeyRequest, KeyResponse, KeyDownRequest, KeyDownResponse, KeyUpRequest, KeyUpResponse, KeyInitRequest, KeyInitResponse, MouseRequest, MouseResponse, MouseAction, Coordinate
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, window: WindowSpecification, keys_map: dict[Key, str]) -> None:
        super().__init__()
        self.window = window
        self.keys_map = keys_map

    # This is the init function that is called each time the bot connects to your service.
    def Init(self, request: KeyInitRequest, context):
        # This is a seed generated automatically by the bot for the first time the bot is run.
        # The seed is saved in the database and reused again later.
        # If you do not wish to use the bot provided delay for key down press, you can use this
        # seed for generating delay timing. The seed is a 32 bytes array.
        # self.seed = request.seed

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

        # pywinauto mouse requires absolute screen coordinate.
        #
        # Case 1: pyautogui input server is in the same PC as bot. Just use Coordinate.Screen is
        # enough.

        # Case 2: pyautogui input server is in a different PC than the bot. This case can be
        # problematic depending on your setup. For instance, if you use GF Now or
        # Moonlight/Sunshine, when running the game, there are no "status bars" or other UI areas.
        # Your game will always show without any kind of border/bars that might inset
        # the actual game. But if you run your game in something like a VM, the VM can has these
        # bars and the bot always capture the full VM app and not just the game being shown. So
        # you need to subtract the coordinates by some amount until it feels "correct".
        #
        # You need to use Coordinate.Relative for this case.

        # These are for cropping the non-game UI portion of the app the game is running in.
        # For Moonlight/Sunshine, you can leave both values as 0. This method can be unreliable.
        crop_left_px = 4  # Change this until it feels correct
        crop_top_px = 74  # Change this until it feels correct

        game_width = 1366  # Assuming your game is 1366x768 full screen
        game_height = 768  # Assuming your game is 1366x768 full screen
        x = int(((x - crop_left_px) / (width - crop_left_px)) * game_width)
        y = int(((y - crop_top_px) / (height - crop_top_px)) * game_height)

        # Common logics, not very human but just an example
        if action == MouseAction.Move:
            pyautogui.moveTo(x, y)
        elif action == MouseAction.Click:
            pyautogui.click(x, y)
        elif action == MouseAction.ScrollDown:
            pyautogui.moveTo(x, y)
            pyautogui.scroll(-200)

        return MouseResponse()

    def Send(self, request: KeyRequest, context):
        if self.window.has_keyboard_focus():
            # This `key` is an enum representing the key the bot want your customized input to send.
            # You should map this to the key supported by your customized input method.
            key = self.keys_map[request.key]
            # This is key down sleep milliseconds. It is generated automatically by the bot using the
            # above seed. You should use this delay and `time.sleep(delay)` on key down.
            key_down = request.down_ms / 1000.0

            keyboard.send_keys(
                "{" + key + " down}", pause=key_down, vk_packet=False)
            keyboard.send_keys(
                "{" + key + " up}", pause=0, vk_packet=False)

        return KeyResponse()

    def SendUp(self, request: KeyUpRequest, context):
        if self.window.has_keyboard_focus():
            keyboard.send_keys(
                "{" + self.keys_map[request.key] + " up}", pause=0, vk_packet=False)
        return KeyUpResponse()

    def SendDown(self, request: KeyDownRequest, context):
        if self.window.has_keyboard_focus():
            keyboard.send_keys(
                "{" + self.keys_map[request.key] + " down}", pause=0, vk_packet=False)
        return KeyDownResponse()


if __name__ == "__main__":
    window_args = {'class_name': 'MapleStoryClass'}
    window = Application().connect(
        handle=pywinauto.findwindows.find_window(
            **window_args)).window()
    # Generated with ChatGPT, might not be accurate
    keys_map = {
        # Letters
        Key.A: 'a',
        Key.B: 'b',
        Key.C: 'c',
        Key.D: 'd',
        Key.E: 'e',
        Key.F: 'f',
        Key.G: 'g',
        Key.H: 'h',
        Key.I: 'i',
        Key.J: 'j',
        Key.K: 'k',
        Key.L: 'l',
        Key.M: 'm',
        Key.N: 'n',
        Key.O: 'o',
        Key.P: 'p',
        Key.Q: 'q',
        Key.R: 'r',
        Key.S: 's',
        Key.T: 't',
        Key.U: 'u',
        Key.V: 'v',
        Key.W: 'w',
        Key.X: 'x',
        Key.Y: 'y',
        Key.Z: 'z',

        # Digits
        Key.Zero: '0',
        Key.One: '1',
        Key.Two: '2',
        Key.Three: '3',
        Key.Four: '4',
        Key.Five: '5',
        Key.Six: '6',
        Key.Seven: '7',
        Key.Eight: '8',
        Key.Nine: '9',

        # Function Keys
        Key.F1: 'F1',
        Key.F2: 'F2',
        Key.F3: 'F3',
        Key.F4: 'F4',
        Key.F5: 'F5',
        Key.F6: 'F6',
        Key.F7: 'F7',
        Key.F8: 'F8',
        Key.F9: 'F9',
        Key.F10: 'F10',
        Key.F11: 'F11',
        Key.F12: 'F12',

        # Navigation and Controls
        Key.Up: 'UP',
        Key.Down: 'DOWN',
        Key.Left: 'LEFT',
        Key.Right: 'RIGHT',
        Key.Home: 'HOME',
        Key.End: 'END',
        Key.PageUp: 'PGUP',
        Key.PageDown: 'PGDN',
        Key.Insert: 'INSERT',
        Key.Delete: 'DEL',
        Key.Esc: 'ESC',
        Key.Enter: 'ENTER',
        Key.Space: 'SPACE',

        # Modifier Keys
        # control (can also be '{VK_CONTROL}' if needed)
        Key.Ctrl: 'VK_CONTROL',
        Key.Shift: 'VK_SHIFT',  # shift (can also be '{VK_SHIFT}')
        Key.Alt: 'VK_MENU',    # alt (can also be '{VK_MENU}')

        # Punctuation & Special Characters
        Key.Tilde: '`',
        Key.Quote: "'",
        Key.Semicolon: ';',
        Key.Comma: ',',
        Key.Period: '.',
        Key.Slash: '/',
    }

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    add_KeyInputServicer_to_server(KeyInput(window, keys_map), server)
    server.add_insecure_port("[::]:5001")
    server.start()
    print("Server started, listening on 5001")
    server.wait_for_termination()
