//! A request's body written in pieces for the gate (plan 2A, task S1, item 5), the same
//! way for each provider's wire: moved here from the Messages wire when the Google and
//! OpenAI wires came (plan m2c, task P1).
//!
//! **What is checked is what is sent.** Each picture is a piece of its own, prepared
//! ([`Piece::prepared`]) and checked as an image; the sender writes the base64 of exactly
//! that piece's bytes between the words around it (`net::http::post`). No picture is
//! copied, decoded or encoded after it is made a piece. The words are pieces too, split
//! only where a picture stands or the class of the words changes.

use serde_json::Value;
use whizbang_core::capability::{Fault, FaultKind, JudgeRequest, Message, Part, Picture};
use whizbang_core::egress::{DataClass, Piece};

/// What a fault says of a picture that could not be prepared.
pub(crate) const PICTURE: &str = "a picture of the request could not be prepared to be sent";
/// What it says of a part a role does not say.
pub(crate) const NOT_SAID: &str = "the conversation holds a part its role does not say";
/// What it says of an opaque part that is not one block of this adapter.
pub(crate) const OPAQUE: &str = "an opaque part of the conversation is not a block the \
                                 adapter wrote";

/// A request as written: its pieces, the prepared size of each of its pictures in
/// order, and how many messages it holds.
pub(crate) struct Written {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) pictures: Vec<(u32, u32)>,
    pub(crate) messages: u64,
}

impl Written {
    /// The pieces of a body, and how many messages it holds.
    pub(crate) fn of(pieces: Vec<Piece>, messages: u64) -> Written {
        let pictures = pieces
            .iter()
            .filter(|piece| piece.is_picture())
            .map(|piece| size_of(piece.bytes()))
            .collect();
        Written {
            pieces,
            pictures,
            messages,
        }
    }

    /// The bytes of its words: every piece that is not a picture.
    pub(crate) fn word_bytes(&self) -> u64 {
        let words = self.pieces.iter().filter(|piece| !piece.is_picture());
        words.map(|piece| piece.bytes().len() as u64).sum()
    }
}

/// How many pictures a request holds: in its messages and in its tools' results.
pub(crate) fn pictures_in(request: &JudgeRequest) -> usize {
    let parts = request.messages().iter().flat_map(Message::parts);
    parts
        .map(|part| match part {
            Part::Image(_) => 1,
            Part::ToolResult(result) => result.images().len(),
            _ => 0,
        })
        .sum()
}

/// The prepared size of a picture as the encoder wrote it: a JPEG's frame, or a PNG's
/// header.
pub(crate) fn size_of(bytes: &[u8]) -> (u32, u32) {
    if let Some(header) = bytes.get(16..24).filter(|_| bytes.starts_with(b"\x89PNG")) {
        let side = |at: usize| {
            u32::from_be_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        return (side(0), side(4));
    }
    let mut at = 2;
    while let Some([0xff, marker, high, low]) =
        bytes.get(at..at + 4).map(|b| [b[0], b[1], b[2], b[3]])
    {
        if marker == 0xc0 {
            let side = |from: usize| {
                bytes
                    .get(from..from + 2)
                    .map_or(0, |two| u32::from(u16::from_be_bytes([two[0], two[1]])))
            };
            return (side(at + 7), side(at + 5));
        }
        at += 2 + usize::from(u16::from_be_bytes([high, low]));
    }
    (0, 0)
}

/// The media type of a prepared picture: a JPEG, or the PNG of one with clear pixels.
pub(crate) fn media_of(piece: &Piece) -> &'static str {
    if piece.bytes().starts_with(b"\xff\xd8") {
        "image/jpeg"
    } else {
        "image/png"
    }
}

/// A body being written: the words since the last piece, their class, and the pieces
/// so far.
pub(crate) struct Body {
    pub(crate) words: String,
    class: DataClass,
    pub(crate) pieces: Vec<Piece>,
}

impl Body {
    pub(crate) fn new() -> Body {
        Body {
            words: String::new(),
            class: DataClass::Prompts,
            pieces: Vec::new(),
        }
    }

    /// The words so far become a piece of their class.
    fn flush(&mut self) {
        if !self.words.is_empty() {
            let words = std::mem::take(&mut self.words).into_bytes();
            self.pieces.push(Piece::of(self.class, words));
        }
    }

    /// Words of this class.
    pub(crate) fn words(&mut self, class: DataClass, text: &str) {
        if class != self.class {
            self.flush();
            self.class = class;
        }
        self.words.push_str(text);
    }

    /// Words that hold no data, the punctuation and the keys of the body: they go
    /// with the words before them, whatever their class.
    pub(crate) fn glue(&mut self, text: &str) {
        self.words.push_str(text);
    }

    /// A JSON value, written as words of this class.
    pub(crate) fn json(&mut self, class: DataClass, value: &Value) {
        let written = serde_json::to_string(value).unwrap_or_default();
        self.words(class, &written);
    }

    /// A picture, prepared, as a piece of its own between `before` and `after`, which
    /// the provider's wire writes around it; `before` is given the media type. What
    /// closes the picture holds no data: it goes with the words after it.
    pub(crate) fn picture(
        &mut self,
        class: DataClass,
        picture: &Picture,
        long_edge: u32,
        before: impl Fn(&str) -> String,
        after: &str,
    ) -> Result<(), Fault> {
        let piece = Piece::prepared(class, picture, long_edge)
            .map_err(|_| Fault::new(FaultKind::Rejected, PICTURE).not_sent())?;
        self.words(class, &before(media_of(&piece)));
        self.flush();
        self.pieces.push(piece);
        self.class = DataClass::Prompts;
        self.glue(after);
        Ok(())
    }

    pub(crate) fn into_pieces(mut self) -> Vec<Piece> {
        self.flush();
        self.pieces
    }
}

/// Writes items between `[` and `]`, with commas. An item for which nothing is written
/// (`each` answers false) takes no comma.
pub(crate) fn list<T>(
    body: &mut Body,
    items: &[T],
    mut each: impl FnMut(&mut Body, &T) -> Result<bool, Fault>,
) -> Result<(), Fault> {
    body.glue("[");
    let mut first = true;
    for item in items {
        let at = (body.words.len(), body.pieces.len());
        if !first {
            body.glue(",");
        }
        if each(body, item)? {
            first = false;
        } else if (body.words.len(), body.pieces.len()) != at {
            // Nothing was written for it but a comma: take the comma back.
            body.words.pop();
        }
    }
    body.glue("]");
    Ok(())
}

/// The long edge every picture of a request is fitted to: the model's, or, for a request
/// of more than 20 pictures, 2,000 pixels at most (squint's catalogue of the Anthropic
/// model: `many_image_threshold`, `many_image_max_dimension_px`; Google's and OpenAI's
/// own long edges are under it already).
pub(crate) fn long_edge_of(request: &JudgeRequest, long_edge: u32) -> u32 {
    if pictures_in(request) > MANY_PICTURES {
        long_edge.min(MANY_LONG_EDGE)
    } else {
        long_edge
    }
}

/// The pictures above which each is fitted to [`MANY_LONG_EDGE`] at most.
pub(crate) const MANY_PICTURES: usize = 20;
/// The long edge of every picture of a request of more than [`MANY_PICTURES`].
pub(crate) const MANY_LONG_EDGE: u32 = 2_000;
