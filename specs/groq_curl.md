curl "https://api.groq.com/openai/v1/chat/completions" \
  -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${GROQ_API_KEY}" \
  -d '{
         "messages": [
           {
             "role": "user",
             "content": ""
           }
         ],
         "model": "moonshotai/kimi-k2-instruct-0905",
         "temperature": 0.6,
         "max_completion_tokens": 4096,
         "top_p": 1,
         "stream": true,
         "stop": null
       }'
  

Prompt:
Your Task is to Clean this spoken dictation into polished text while preserving the speaker's original meaning and voice. Only return the corrected text. 

- Remove filler words (um, uh, like, you know, so, basically)
  Example: "so um I think we should like focus on the API" → "I think we should focus on the API"

- Handle self-corrections (keep only the final intent)
  Example: "We need to deploy on Friday... no wait, actually Thursday" → "We need to deploy on Thursday"

- Remove accidental repetitions
  Example: "the the database query is slow" → "the database query is slow"

- Add proper punctuation and formatting
  Example: "lets meet at 3 ill send you the agenda" → "Let's meet at 3. I'll send you the agenda."

- Preserve technical terms, acronyms, and domain-specific language exactly as spoken
  Example: "the REST API endpoint" → "the REST API endpoint" (not "rest api")

- Maintain the natural tone and style of the speaker
  Example: If casual: "gonna ship this tomorrow" → "Going to ship this tomorrow"
  Example: If formal: "we shall proceed with implementation" → "We shall proceed with implementation"

- Convert spoken numbers and dates into appropriate written format
  Example: "meet on the twenty third of march" → "meet on the 23rd of March"

Custom Dictionary words you may need to replace:
Sebastian Kouba
HiQ
Parakeet V3
Claude Code
Kattl
Burgi
Sofle
Lily58

Original dictation: