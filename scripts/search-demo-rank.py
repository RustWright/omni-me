#!/usr/bin/env python3
"""Rank the search demo's made-up records with the server's exact embedding model.

    pip install fastembed
    scripts/search-demo-rank.py [fixtures/search_demo_corpus.json]

Prints the top five records per question, with cosine scores and the content words
each shares with the question. The Assistant demo's search answers cite the record
ranked first here (`OMNI_DEMO_SCENARIO=search` in the frontend's mock bridge).

The server (fastembed-rs 6.0.3, `bge-small-en-v1.5`) loads Xenova/bge-small-en-v1.5
`onnx/model.onnx` with CLS pooling and normalised output. Python fastembed's built-in
name for that model is a quantized copy, so the exact file is registered here.
Passages are embedded raw and questions get BGE's query prefix, as in
`core/src/assistant/embedding.rs`.
"""
import json
import re
import sys
import unicodedata

import numpy as np
from fastembed import TextEmbedding
from fastembed.common.model_description import ModelSource, PoolingType

MODEL = "omni/bge-small-en-v1.5-xenova"
QUERY_PREFIX = "Represent this sentence for searching relevant passages: "

# `core/src/assistant/query_text.rs` STOPWORDS, verbatim.
STOPWORDS = set("""a an the this that these those some any each
i me my mine you your yours he him his she her hers it its we us our ours they them their theirs
who whom whose what which when where why how
am is are was were be been being do does did have has had should would could
of in on at to for with from by about as into over and or but if than then so
again just very there here please tell""".split())


def fold(token):
    return unicodedata.normalize("NFKD", token).encode("ascii", "ignore").decode().lower()


def words(text):
    # The `omni_text` analyzer splits on whitespace and character class, then folds
    # case and accents. Only letter and digit runs can match a question's word.
    return {fold(t) for t in re.findall(r"[^\W\d_]+|\d+", text)}


def content_words(question):
    kept = [t for t in question.split() if fold(t.strip("?!.,;:'\"")) not in STOPWORDS]
    return words(" ".join(kept or question.split()))


def main(corpus_path):
    corpus = json.load(open(corpus_path))
    TextEmbedding.add_custom_model(
        model=MODEL,
        pooling=PoolingType.CLS,
        normalization=True,
        sources=ModelSource(hf="Xenova/bge-small-en-v1.5"),
        dim=384,
        model_file="onnx/model.onnx",
    )
    model = TextEmbedding(model_name=MODEL)
    records = corpus["records"]
    passages = np.array(list(model.embed([r["text"] for r in records])))
    queries = model.embed([QUERY_PREFIX + q for q in corpus["questions"]])

    for question, vector in zip(corpus["questions"], queries):
        scores = passages @ vector
        asked = content_words(question)
        print(f"\n{question}   content words: {sorted(asked)}")
        for i in sorted(range(len(records)), key=lambda i: -scores[i])[:5]:
            shared = sorted(asked & words(records[i]["text"]))
            print(f"  {scores[i]:.3f}  {records[i]['id']}  shared={shared}  {records[i]['text']}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "fixtures/search_demo_corpus.json")
