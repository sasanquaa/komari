## Customize Input
The bot currently does not use advanced input method such as a driver like `Interception` but only a normal Win32 API `SendInput`, so you should use at least be aware/cautious and use the bot default input mode at your own risk. If you want more security, customizing the bot with hardware input (KMBox, Arduino,...) using `Rpc` method provided in the `Settings` tab is recommended. However, this currently requires some scripting:
  - Use the language of your choice to write, host it and provide the server URL to the bot as long as you can generate gRPC stubs
  - Check this [example](https://github.com/sasanquaa/maple-bot/tree/master/examples/python):
      - Note that this example is tested on the same PC so `http://localhost:5001` is used
      - If you host the input server on the game PC and the bot runs on a different PC, you need to change the IP, port-forward, etc.. so that the bot can connect to the input server on the game PC
      - Downloading `app-debug` version if needed to check if the bot connects successfully by looking at the log
  - (Just an idea, not tested) For local PC, using Unix socket can likely improve input latency instead of gRPC default HTTP

![Customize Input](https://github.com/sasanquaa/komari/blob/master/.github/images/customize_input.png?raw=true)
