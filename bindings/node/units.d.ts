/** UTF-8 byte offset into `Document.text`. A plain number at runtime; the brand prevents mixing units. */
export type ByteOffset = number & { readonly __byteOffset: unique symbol };
/** Unicode code-point offset into `Document.text`, like spaCy `Token.idx`. A plain number at runtime. */
export type CodePointOffset = number & { readonly __codePointOffset: unique symbol };
/** UTF-16 code-unit offset into `Document.text`, for JavaScript `text.slice`. A plain number at runtime. */
export type Utf16Offset = number & { readonly __utf16Offset: unique symbol };
/** Zero-based position in `Document.tokens`. A plain number at runtime. */
export type TokenIndex = number & { readonly __tokenIndex: unique symbol };
