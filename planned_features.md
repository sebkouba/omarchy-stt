Sentence embeddings for paragraph breaks - Models like all-MiniLM-L6-v2 are tiny (~80MB) and blazing fast:

Embed each sentence
Compare cosine similarity between consecutive sentences
Low similarity = probable paragraph break
Runs in milliseconds on your M1

