Notes from reading "Attention Is All You Need".

## Self-attention

Self-attention lets every token look at every other token in the sequence. Each token is projected into a query, a key and a value; the attention weights are the softmax of query-key dot products, scaled by the square root of the key dimension.

## Positional encoding

Transformers have no recurrence, so sine and cosine positional encodings are added to the embeddings to give the model word order.

## Cost

Attention is quadratic in sequence length, which limits the context window.
