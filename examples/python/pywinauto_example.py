import asyncio
import ctypes
import grpc
import pyautogui
import pywinauto

from pywinauto.application import Application
from pywinauto import WindowSpecification, keyboard

from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server
from input_pb2 import (
    Key,
    KeyRequest,
    KeyResponse,
    KeyDownRequest,
    KeyDownResponse,
    KeyUpRequest,
    KeyUpResponse,
    KeyInitRequest,
    KeyInitResponse,
    MouseRequest,
    MouseResponse,
    MouseAction,
    Coordinate,
    KeyState,
    KeyStateRequest,
    KeyStateResponse,
)

user32 = ctypes.windll.user32


class KeyInput(KeyInputServicer):
    def __init__(self, window: WindowSpecification, keys_map: [Key, str]):
        self.window = window
        self.keys_map = keys_map
        self.timers_map: [Key, asyncio.Handle] = {}
        self.loop = asyncio.get_event_loop()

    async def Init(self, request: KeyInitRequest, context):
        return KeyInitResponse(mouse_coordinate=Coordinate.Relative)

    async def KeyState(self, request: KeyStateRequest, context):
        is_down = (user32.GetAsyncKeyState(
            self.keys_map[request.key]) & 0x8000) != 0
        return KeyStateResponse(
            state=KeyState.Pressed if is_down else KeyState.Released
        )

    async def SendMouse(self, request: MouseRequest, context):
        width = request.width
        height = request.height
        x = request.x
        y = request.y
        action = request.action

        crop_left_px = 0
        crop_top_px = 30

        game_width = 1366
        game_height = 768

        x = int(((x - crop_left_px) / (width - crop_left_px)) * game_width)
        y = int(((y - crop_top_px) / (height - crop_top_px)) * game_height)

        if action == MouseAction.Move:
            pyautogui.moveTo(x, y)

        elif action == MouseAction.Click:
            pyautogui.moveTo(x, y)
            await asyncio.sleep(0.08)
            pyautogui.click()

        elif action == MouseAction.ScrollDown:
            pyautogui.moveTo(x, y)
            await asyncio.sleep(0.08)
            pyautogui.scroll(-200)

        return MouseResponse()

    async def Send(self, request: KeyRequest, context):
        if not self.window.has_keyboard_focus():
            return KeyResponse()

        key = request.key
        delay = request.down_ms / 1000.0

        handle = self.timers_map.get(key)

        if handle is None:
            self._send_down(key)

            def release():
                self._send_up(key)
                self.timers_map.pop(key, None)

            handle = self.loop.call_later(delay, release)
            self.timers_map[key] = handle

        return KeyResponse()

    async def SendDown(self, request: KeyDownRequest, context):
        if self.window.has_keyboard_focus():
            self._send_down(request.key)
        return KeyDownResponse()

    async def SendUp(self, request: KeyUpRequest, context):
        if self.window.has_keyboard_focus():
            self._send_up(request.key)

            handle = self.timers_map.pop(request.key, None)
            if handle:
                handle.cancel()

        return KeyUpResponse()

    def _send_up(self, key: Key):
        keyboard.send_keys(
            "{" + self.keys_map[key] + " up}", pause=0, vk_packet=False
        )

    def _send_down(self, key: Key):
        keyboard.send_keys(
            "{" + self.keys_map[key] + " down}", pause=0, vk_packet=False
        )


async def serve():
    window_args = {"class_name": "MapleStoryClass"}

    window = (
        Application()
        .connect(handle=pywinauto.findwindows.find_window(**window_args))
        .window()
    )

    keys_map = {
        Key.A: "a", Key.B: "b", Key.C: "c", Key.D: "d",
        Key.E: "e", Key.F: "f", Key.G: "g", Key.H: "h",
        Key.I: "i", Key.J: "j", Key.K: "k", Key.L: "l",
        Key.M: "m", Key.N: "n", Key.O: "o", Key.P: "p",
        Key.Q: "q", Key.R: "r", Key.S: "s", Key.T: "t",
        Key.U: "u", Key.V: "v", Key.W: "w", Key.X: "x",
        Key.Y: "y", Key.Z: "z",

        Key.Zero: "0", Key.One: "1", Key.Two: "2",
        Key.Three: "3", Key.Four: "4", Key.Five: "5",
        Key.Six: "6", Key.Seven: "7", Key.Eight: "8",
        Key.Nine: "9",

        Key.F1: "F1", Key.F2: "F2", Key.F3: "F3",
        Key.F4: "F4", Key.F5: "F5", Key.F6: "F6",
        Key.F7: "F7", Key.F8: "F8", Key.F9: "F9",
        Key.F10: "F10", Key.F11: "F11", Key.F12: "F12",

        Key.Up: "UP", Key.Down: "DOWN", Key.Left: "LEFT", Key.Right: "RIGHT",
        Key.Home: "HOME", Key.End: "END",
        Key.PageUp: "PGUP", Key.PageDown: "PGDN",
        Key.Insert: "INSERT", Key.Delete: "DEL",

        Key.Esc: "ESC", Key.Enter: "ENTER", Key.Space: "SPACE",

        Key.Ctrl: "VK_CONTROL", Key.Shift: "VK_SHIFT", Key.Alt: "VK_MENU",

        Key.Tilde: "`", Key.Quote: "'",
        Key.Semicolon: ";", Key.Comma: ",",
        Key.Period: ".", Key.Slash: "/",
    }

    server = grpc.aio.server()
    add_KeyInputServicer_to_server(KeyInput(window, keys_map), server)

    server.add_insecure_port("[::]:5001")

    await server.start()
    print("Server started, listening on 5001")

    await server.wait_for_termination()


if __name__ == "__main__":
    asyncio.run(serve())
