import asyncio
import pyautogui
import grpc
import serial

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

KEY_DOWN = 1
KEY_UP = 2
MOUSE_MOVE = 3
MOUSE_CLICK = 4
MOUSE_SCROLL = 5


class KeyInput(KeyInputServicer):
    def __init__(self, keys_map: [Key, int], serial_port):
        self.keys_map = keys_map
        self.serial = serial_port
        self.timers_map: [Key, asyncio.Handle] = {}
        self.key_down = {}
        self.loop = asyncio.get_event_loop()

    async def Init(self, request: KeyInitRequest, context):
        return KeyInitResponse(mouse_coordinate=Coordinate.Screen)

    async def SendMouse(self, request: MouseRequest, context):
        x = request.x
        y = request.y
        action = request.action

        position = pyautogui.position()
        dx = x - position.x
        dy = y - position.y

        dx_bytes = dx.to_bytes(2, byteorder="little", signed=True)
        dy_bytes = dy.to_bytes(2, byteorder="little", signed=True)

        if action == MouseAction.Move:
            self.serial.write(bytes([MOUSE_MOVE]) + dx_bytes + dy_bytes)

        elif action == MouseAction.Click:
            self.serial.write(bytes([MOUSE_MOVE]) + dx_bytes + dy_bytes)
            await asyncio.sleep(0.08)
            self.serial.write(bytes([MOUSE_CLICK]))

        elif action == MouseAction.ScrollDown:
            scroll_bytes = int(1000).to_bytes(
                2, byteorder="little", signed=True)
            self.serial.write(bytes([MOUSE_MOVE]) + dx_bytes + dy_bytes)
            await asyncio.sleep(0.08)
            self.serial.write(bytes([MOUSE_SCROLL]) + scroll_bytes)

        return MouseResponse()

    async def KeyState(self, request: KeyStateRequest, context):
        down = self.key_down.get(request.key, False)
        return KeyStateResponse(
            state=KeyState.Pressed if down else KeyState.Released
        )

    async def Send(self, request: KeyRequest, context):
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
        key = request.key
        self._send_down(key)
        return KeyDownResponse()

    async def SendUp(self, request: KeyUpRequest, context):
        key = request.key
        self._send_up(key)

        handle = self.timers_map.pop(key, None)
        if handle:
            handle.cancel()

        return KeyUpResponse()

    def _send_up(self, key: Key):
        self.serial.write(bytes([KEY_UP, self.keys_map[key]]))
        self.key_down[key] = False

    def _send_down(self, key: Key):
        self.serial.write(bytes([KEY_DOWN, self.keys_map[key]]))
        self.key_down[key] = True


async def serve():
    serial_port = serial.Serial("COM6")

    keys_map = {
        Key.A: ord('a'), Key.B: ord('b'), Key.C: ord('c'),
        Key.D: ord('d'), Key.E: ord('e'), Key.F: ord('f'),
        Key.G: ord('g'), Key.H: ord('h'), Key.I: ord('i'),
        Key.J: ord('j'), Key.K: ord('k'), Key.L: ord('l'),
        Key.M: ord('m'), Key.N: ord('n'), Key.O: ord('o'),
        Key.P: ord('p'), Key.Q: ord('q'), Key.R: ord('r'),
        Key.S: ord('s'), Key.T: ord('t'), Key.U: ord('u'),
        Key.V: ord('v'), Key.W: ord('w'), Key.X: ord('x'),
        Key.Y: ord('y'), Key.Z: ord('z'),

        Key.Zero: ord('0'), Key.One: ord('1'), Key.Two: ord('2'),
        Key.Three: ord('3'), Key.Four: ord('4'), Key.Five: ord('5'),
        Key.Six: ord('6'), Key.Seven: ord('7'), Key.Eight: ord('8'),
        Key.Nine: ord('9'),

        Key.F1: 0xC2, Key.F2: 0xC3, Key.F3: 0xC4,
        Key.F4: 0xC5, Key.F5: 0xC6, Key.F6: 0xC7,
        Key.F7: 0xC8, Key.F8: 0xC9, Key.F9: 0xCA,
        Key.F10: 0xCB, Key.F11: 0xCC, Key.F12: 0xCD,

        Key.Up: 0xDA, Key.Down: 0xD9, Key.Left: 0xD8, Key.Right: 0xD7,
        Key.Home: 0xD2, Key.End: 0xD5,
        Key.PageUp: 0xD3, Key.PageDown: 0xD6,
        Key.Insert: 0xD1, Key.Delete: 0xD4,

        Key.Esc: 0xB1, Key.Enter: 0xE0, Key.Space: ord(' '),

        Key.Ctrl: 0x80, Key.Shift: 0x81, Key.Alt: 0x82,

        Key.Tilde: ord('`'), Key.Quote: ord("'"),
        Key.Semicolon: ord(';'), Key.Comma: ord(','),
        Key.Period: ord('.'), Key.Slash: ord('/'),
    }

    server = grpc.aio.server()
    add_KeyInputServicer_to_server(KeyInput(keys_map, serial_port), server)

    server.add_insecure_port("[::]:5001")

    await server.start()
    print("Server started, listening on 5001")

    await server.wait_for_termination()


if __name__ == "__main__":
    asyncio.run(serve())
