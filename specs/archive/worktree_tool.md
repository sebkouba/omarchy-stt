Our dictation app supports tool calls via an LLM. like:
    {
      "name": "open_url",
      "description": "Open any URL in Firefox. Call when user says 'open github.com', 'go to localhost 3000', 'browse to anthropic.com', 'show me stackoverflow.com/questions/123'. Works with any domain, localhost with port numbers, or full URLs.",
      "command": "firefox",
      "args": ["--new-window", "{url}"],
      "parameters": {
        "url": {
          "type": "string",
          "description": "Full URL including protocol (e.g., 'https://github.com', 'http://localhost:3000') OR just domain (e.g., 'github.com', 'anthropic.com'). If user says 'localhost 3000' convert to 'http://localhost:3000'."
        }
      }
    }

i want to add a tool, that uses git worktrees to set up a new branch in a certain project. claude code should then launch to implement a feature that is part of the voice transcription. Maybe this skill or tool will start the process and then I have to go into the terminal window where it launched to finish the instructions and interact with it. But I guess this will work for simple features. 

the bash script our tool should launch is called claude-worktree.sh at project root. 

double check that script then implemente the tool. 

i think the script is missing a mechanism to decide which project / path to launch this in. lets use a projects.md file in our config dir where i list project with short desription and path. will that work? that info needs to be part of the process. that decision is a separate call, isn't it? I guess for now it would have to be for a predetermined project like the one we are in. or can we chain tool calls?