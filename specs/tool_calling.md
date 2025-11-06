we have this repo: 
https://github.com/64bit/async-openai/tree/main

This should be a Rust crate that we can use to call the kimmi K2 endpoint, which I want to do for tool calls to switch my display background LEDs on and off. the repo has examples. figure out the right example to use. lets convert our app to use this library then add a tool calling options for when i want to turn my leds on or off with these calls
on:
curl -s -X POST "http://192.168.2.40/json/state" -d '{"on":true,"v":true}' -H "Content-Type: application/json"
}

off:
curl -s -X POST "http://192.168.2.40/json/state" -d '{"on":false}' -H "Content-Type: application/json"