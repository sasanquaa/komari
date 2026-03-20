import asyncio
import kmNet
import grpc
import pyautogui

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
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, keys_map: [Key, int]):
        self.keys_map = keys_map
        self.timers_map: [Key, asyncio.Handle] = {}
        self.seed = None
        self.loop = asyncio.get_event_loop()

    async def Init(self, request: KeyInitRequest, context):
        self.seed = request.seed
        return KeyInitResponse(mouse_coordinate=Coordinate.Relative)

    async def KeyState(self, request: KeyStateRequest, context):
        key = self.keys_map[request.key]
        if kmNet.isdown_keyboard(key) == 1:
            return KeyStateResponse(state=KeyState.Pressed)
        else:
            return KeyStateResponse(state=KeyState.Released)

    async def SendMouse(self, request: MouseRequest, context):
        width = request.width
        height = request.height
        x = request.x
        y = request.y
        action = request.action

        screen_width, screen_height = pyautogui.size()
        position = pyautogui.position()

        crop_left_px = 0
        crop_top_px = 30

        scaled_x = int(
            ((x - crop_left_px) / (width - crop_left_px)) * screen_width)
        scaled_y = int(
            ((y - crop_top_px) / (height - crop_top_px)) * screen_height)

        dx = scaled_x - position.x
        dy = scaled_y - position.y

        if action == MouseAction.Move:
            kmNet.move(dx, dy)
        elif action == MouseAction.Click:
            kmNet.move(dx, dy)
            kmNet.mouse(1, 0, 0, 0)
            kmNet.mouse(0, 0, 0, 0)
        elif action == MouseAction.ScrollDown:
            kmNet.move(dx, dy)
            kmNet.mouse(0, 0, 0, -1)

        return MouseResponse()

    async def Send(self, request: KeyRequest, context):
        key = request.key
        key_mapped = self.keys_map[key]
        delay = request.down_ms / 1000.0

        handle = self.timers_map.get(key)

        if handle is None:
            kmNet.keydown(key_mapped)

            def release():
                kmNet.keyup(key_mapped)
                self.timers_map.pop(key, None)

            handle = self.loop.call_later(delay, release)
            self.timers_map[key] = handle

        return KeyResponse()

    async def SendUp(self, request: KeyUpRequest, context):
        key = request.key
        kmNet.keyup(self.keys_map[key])

        handle = self.timers_map.pop(key, None)
        if handle:
            handle.cancel()

        return KeyUpResponse()

    async def SendDown(self, request: KeyDownRequest, context):
        key = request.key
        kmNet.keydown(self.keys_map[key])
        return KeyDownResponse()


async def serve():
    kmNet.init("192.168.2.188", "8704", "33005C53")
    kmNet.monitor(1)

    keys_map = {
        **{Key.Value(Key.Name(i)): 4 + i for i in range(26)},
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

    server = grpc.aio.server()
    add_KeyInputServicer_to_server(KeyInput(keys_map), server)

    server.add_insecure_port("[::]:5001")

    await server.start()
    print("Server started, listening on 5001")

    await server.wait_for_termination()


if __name__ == "__main__":
    asyncio.run(serve())
