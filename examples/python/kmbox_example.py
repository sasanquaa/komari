import kmNet
import grpc
import time

from random import random
from concurrent import futures
# The two imports below is generated from:
# python -m grpc_tools.protoc --python_out=. --pyi_out=. --grpc_python_out=. -I../../backend/proto ../..
# /backend/proto/input.proto
from input_pb2 import Key, KeyRequest, KeyResponse, KeyDownRequest, KeyDownResponse, KeyUpRequest, KeyUpResponse, KeyInitRequest, KeyInitResponse
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, keys_map: dict[Key, int]) -> None:
        super().__init__()
        self.keys_map = keys_map

    # This is the init function that is called each time the bot connects to your service.
    def Init(self, request: KeyInitRequest, context):
        # This is a seed generated automatically by the bot for the first time the bot is run.
        # The seed is saved in the database and reused again later.
        # If you do not wish to use the bot provided delay for key down press, you can use this
        # seed for generating delay timing. The seed is a 32 bytes array.
        # self.seed = request.seed

        return KeyInitResponse()

    def Send(self, request: KeyRequest, context):
        # This `key` is an enum representing the key the bot want your customized input to send.
        # You should map this to the key supported by your customized input method.
        key = self.keys_map[request.key]
        # This is key down sleep milliseconds. It is generated automatically by the bot using the
        # above seed. You should use this delay and `time.sleep(delay)` on key down.
        key_down_ms = request.down_ms

        kmNet.keydown(key)
        time.sleep(key_down_ms)
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
