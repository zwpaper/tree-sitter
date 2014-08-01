#include "tree_sitter/runtime.h"
#include "tree_sitter/parser.h"
#include "runtime/tree.h"
#include "runtime/node.h"
#include "runtime/parser.h"
#include "runtime/string_input.h"

struct TSDocument {
  TSParser parser;
  const TSTree *tree;
  TSInput input;
};

TSDocument *ts_document_make() {
  TSDocument *document = malloc(sizeof(TSDocument));
  *document = (TSDocument) {
    .input = (TSInput) { .data = NULL, .read_fn = NULL, .seek_fn = NULL },
    .tree = NULL
  };
  return document;
}

void ts_document_free(TSDocument *document) {
  ts_parser_destroy(&document->parser);
  if (document->input.release_fn)
    document->input.release_fn(document->input.data);
  free(document);
}

void ts_document_set_language(TSDocument *document,
                              const TSLanguage *language) {
  ts_parser_destroy(&document->parser);
  document->parser = ts_parser_make(language);
  if (document->input.read_fn)
    document->tree = ts_parser_parse(&document->parser, document->input, NULL);
}

const TSTree *ts_document_tree(const TSDocument *document) {
  return document->tree;
}

const char *ts_document_string(const TSDocument *document) {
  return ts_tree_string(document->tree,
                        document->parser.language->symbol_names);
}

void ts_document_set_input(TSDocument *document, TSInput input) {
  document->input = input;
  if (document->parser.language)
    document->tree = ts_parser_parse(&document->parser, document->input, NULL);
}

void ts_document_edit(TSDocument *document, TSInputEdit edit) {
  document->tree = ts_parser_parse(&document->parser, document->input, &edit);
}

const char *ts_document_symbol_name(const TSDocument *document,
                                    const TSTree *tree) {
  return document->parser.language->symbol_names[tree->symbol];
}

void ts_document_set_input_string(TSDocument *document, const char *text) {
  ts_document_set_input(document, ts_string_input_make(text));
}

TSNode *ts_document_root_node(const TSDocument *document) {
  if (document->tree)
    return ts_node_make_root(document->tree,
                             document->parser.language->symbol_names);
  else
    return NULL;
}

TSNode *ts_document_get_node(const TSDocument *document, size_t pos) {
  TSNode *root = ts_document_root_node(document);
  TSNode *result = ts_node_leaf_at_pos(root, pos);
  ts_node_release(root);
  return result;
}
