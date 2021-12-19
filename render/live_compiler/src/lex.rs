use{
    makepad_id_macros::*,
    makepad_math::{
        colorhex::hex_bytes_to_u32
    },
    makepad_live_tokenizer::{LiveId, Delim},
    crate::{
        live_ptr::{LiveFileId},
        live_error::{LiveError, LiveErrorOrigin},
        span::{Span, TextPos},
        live_token::{LiveToken,  TokenWithSpan},
    }
};

#[derive(Clone)]
pub struct Lex<C> {
    chars: C,
    file_id: LiveFileId,
    temp_string: String,
    temp_hex: Vec<u8>,
    strings: Vec<char>,
    group_stack: Vec<char>,
    ch_0: char,
    ch_1: char,
    index: usize,
    pos: TextPos,
    is_done: bool,
}


impl<C> Lex<C>
where
C: Iterator<Item = char>,
{
    
    fn read_token_with_span(&mut self) -> Result<TokenWithSpan, LiveError> {
        let span = self.begin_span();
        loop {
            self.skip_chars_while( | ch | ch.is_ascii_whitespace());
            match (self.ch_0, self.ch_1) {
                ('/', '*') => {
                    self.skip_two_chars();
                    loop {
                        match (self.ch_0, self.ch_1) {
                            ('\0', _) => {
                                return Err(span.error(self, "unterminated block comment".into()));
                            }
                            ('*', '/') => {
                                self.skip_two_chars();
                                break;
                            }
                            _ => {
                                self.skip_char();
                            }
                        }
                    }
                }
                ('/', '/') => {
                    self.skip_two_chars();
                    loop {
                        match (self.ch_0, self.ch_1) {
                            ('\n', _) => {
                                self.skip_char();
                                break;
                            }
                            ('\r', '\n') => {
                                self.skip_two_chars();
                                break;
                            }
                            _ => {
                                self.skip_char();
                            }
                        }
                    }
                }
                _ => break,
            }
        }
        
        
        let span = self.begin_span();
        let token = match (self.ch_0, self.ch_1) {
            ('"', _) => { // read a string
                self.skip_char();
                //let mut string = String::new();
                let start = self.strings.len();
                while let Some(ch) = self.read_char_if( | ch | ch != '"' && ch != '\0') {
                    self.strings.push(ch)
                }
                if self.ch_0 == '"' {
                    self.skip_char();
                }
                LiveToken::String {
                    index: start as u32,
                    len: (self.strings.len() - start) as u32
                }
            }
            ('\0', _) => LiveToken::Eof,
            ('!', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( !=))
            }
            ('!', _) => {
                self.skip_char();
                LiveToken::Punct(id!(!))
            }
            ('#', _) => {
                self.skip_char();
                self.temp_hex.truncate(0);
                if self.ch_0 == 'x' {
                    self.skip_char();
                }
                while let Some(ch) = self.read_char_if( | ch | ch.is_ascii_hexdigit()) {
                    self.temp_hex.push(ch as u8)
                }
                if let Ok(color) = hex_bytes_to_u32(&self.temp_hex) {
                    LiveToken::Color(color)
                }
                else {
                    return Err(span.error(self, "Cannot parse color".into()));
                }
            }
            ('&', '&') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( &&))
            }
            
            ('*', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( *=))
            }
            ('*', _) => {
                self.skip_char();
                LiveToken::Punct(id!(*))
            }
            ('+', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( +=))
            }
            ('+', _) => {
                self.skip_char();
                LiveToken::Punct(id!( +))
            }
            (',', _) => {
                self.skip_char();
                LiveToken::Punct(id!(,))
            }
            ('-', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( -=))
            }
            ('-', '>') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( ->))
            }
            ('-', '.') => {
                self.temp_string.truncate(0);
                self.skip_two_chars();
                self.temp_string.push('-');
                self.temp_string.push('0');
                self.temp_string.push('.');
                self.read_chars_while( | ch | ch.is_ascii_digit());
                LiveToken::Float(self.temp_string.parse::<f64>().unwrap())
            }
            ('-', ch) | ('.', ch) | (ch, _) if ch.is_ascii_digit() => {
                self.temp_string.truncate(0);
                if self.ch_0 == '-' {
                    self.skip_char();
                    self.temp_string.push('-');
                }
                self.read_chars_while( | ch | ch.is_ascii_digit());
                let has_frac_part = if let Some(ch) = self.read_char_if( | ch | ch == '.') {
                    self.temp_string.push(ch);
                    self.read_chars_while( | ch | ch.is_ascii_digit());
                    true
                } else {
                    false
                };
                let has_exp_part = if let Some(ch) = self.read_char_if( | ch | ch == 'E' || ch == 'e')
                {
                    self.temp_string.push(ch);
                    if let Some(ch) = self.read_char_if( | ch | ch == '+' || ch == '-') {
                        self.temp_string.push(ch);
                    }
                    if let Some(ch) = self.read_char_if( | ch | ch.is_ascii_digit()) {
                        self.temp_string.push(ch);
                        self.read_chars_while( | ch | ch.is_ascii_digit());
                    } else {
                        return Err(span.error(self, "missing float exponent".into()));
                    }
                    true
                } else {
                    false
                };
                if has_frac_part || has_exp_part {
                    LiveToken::Float(self.temp_string.parse::<f64>().unwrap())
                } else {
                    LiveToken::Int(self.temp_string.parse::<i64>().map_err( | _ | {
                        span.error(self, "overflowing integer literal".into())
                    }) ?)
                }
            }
            ('-', _) => {
                self.skip_char();
                LiveToken::Punct(id!(-))
            }
            ('.', '.') => {
                self.skip_two_chars();
                LiveToken::Punct(id!(..))
            }
            ('.', _) => {
                self.skip_char();
                LiveToken::Punct(id!(.))
            }
            ('/', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( /=))
            }
            ('/', _) => {
                self.skip_char();
                LiveToken::Punct(id!( /))
            }
            (':', ':') => {
                self.skip_two_chars();
                LiveToken::Punct(id!(::))
            }
            (':', _) => {
                self.skip_char();
                LiveToken::Punct(id!(:))
            }
            (';', _) => {
                self.skip_char();
                LiveToken::Punct(id!(;))
            }
            ('<', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( <=))
            }
            ('<', _) => {
                self.skip_char();
                LiveToken::Punct(id!(<))
            }
            ('=', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( ==))
            }
            ('=', '>') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( =>))
            }
            ('=', _) => {
                self.skip_char();
                LiveToken::Punct(id!( =))
            }
            ('>', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( >=))
            }
            ('>', _) => {
                self.skip_char();
                LiveToken::Punct(id!(>))
            }
            ('?', _) => {
                self.skip_char();
                LiveToken::Punct(id!( ?))
            }
            ('&', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( &=))
            }
            ('&', _) => {
                self.skip_char();
                LiveToken::Punct(id!(&))
            }
            ('|', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( |=))
            }
            ('|', _) => {
                self.skip_char();
                LiveToken::Punct(id!( |))
            }
            ('^', '=') => {
                self.skip_two_chars();
                LiveToken::Punct(id!( ^=))
            }
            ('^', _) => {
                self.skip_char();
                LiveToken::Punct(id!( ^))
            }
            (ch, _) if ch.is_ascii_alphabetic() || ch == '_' => {
                // you cant give ids a _ at start or _ at end or __ in the middle
                self.temp_string.truncate(0);
                let first_ch = self.read_char();
                
                self.temp_string.push(first_ch);
                
                // this underscore filtering is because otherwise transpiling to glsl would
                // be unable to use the real identifiers in the generated code.
                // this makes it much harder to read and debug.
                let mut last_ch = '\0';
                let mut double_underscore = false;
                self.read_chars_while( | ch | {
                    if ch == '_' && last_ch == '_' {double_underscore = true};
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        last_ch = ch;
                        true
                    }
                    else {
                        false
                    }
                });
                
                if first_ch == '_' || last_ch == '_' {
                    return Err(span.error(self, format!("Id's cannot start or end with an underscore {}", self.temp_string).into()));
                }
                if double_underscore {
                    return Err(span.error(self, format!("Id's cannot contain double underscores {}", self.temp_string).into()));
                }
                
                match self.temp_string.as_str() {
                    "true" => LiveToken::Bool(true),
                    "false" => LiveToken::Bool(false),
                    _ => {
                        match LiveId::from_str(&self.temp_string) {
                            Err(collide) => return Err(span.error(self, format!("Id has collision {} with {}, please rename one of them", self.temp_string, collide).into())),
                            Ok(id) => LiveToken::Ident(id)
                        }
                    }
                }
            }
            ('(', _) => {
                self.skip_char();
                self.group_stack.push(')');
                LiveToken::Open(Delim::Paren)
            }
            (')', _) => {
                if let Some(exp) = self.group_stack.pop() {
                    if exp != ')' {
                        return Err(span.error(self, format!("Expected {} but got )", exp).into()));
                    }
                }
                else {
                    return Err(span.error(self, "Got ) but no matching (".into()));
                }
                self.skip_char();
                LiveToken::Close(Delim::Paren)
            }
            ('[', _) => {
                self.skip_char();
                self.group_stack.push(']');
                LiveToken::Open(Delim::Bracket)
            }
            (']', _) => {
                if let Some(exp) = self.group_stack.pop() {
                    if exp != ']' {
                        return Err(span.error(self, format!("Expected {} but got ]", exp).into()));
                    }
                }
                else {
                    return Err(span.error(self, "Got ] but no matching [".into()));
                }
                self.skip_char();
                LiveToken::Close(Delim::Bracket)
            }
            ('{', _) => {
                self.skip_char();
                self.group_stack.push('}');
                LiveToken::Open(Delim::Brace)
            }
            ('}', _) => {
                if let Some(exp) = self.group_stack.pop() {
                    if exp != '}' {
                        return Err(span.error(self, format!("Expected {} but got }}", exp).into()));
                    }
                }
                else {
                    return Err(span.error(self, "Got } but no matching {".into()));
                }
                self.skip_char();
                LiveToken::Close(Delim::Brace)
            }
            _ => {
                return Err(span.error(self, format!("unexpected character `{}`", self.ch_0).into()))
            }
        };
        Ok(span.token(self, token))
    }
    
    fn read_chars_while<P>(&mut self, mut pred: P)
    where
    P: FnMut(char) -> bool,
    {
        while let Some(ch) = self.read_char_if(&mut pred) {
            self.temp_string.push(ch);
        }
    }
    
    fn read_char_if<P>(&mut self, pred: P) -> Option<char>
    where
    P: FnOnce(char) -> bool,
    {
        if pred(self.ch_0) {
            Some(self.read_char())
        } else {
            None
        }
    }
    
    fn read_char(&mut self) -> char {
        let ch = self.ch_0;
        self.skip_char();
        ch
    }
    
    fn skip_chars_while<P>(&mut self, mut pred: P)
    where
    P: FnMut(char) -> bool,
    {
        while self.skip_char_if(&mut pred) {}
    }
    
    fn skip_char_if<P>(&mut self, pred: P) -> bool
    where
    P: FnOnce(char) -> bool,
    {
        if pred(self.ch_0) {
            self.skip_char();
            true
        } else {
            false
        }
    }
    
    fn skip_char(&mut self) {
        if self.ch_0 == '\n'{
            self.pos.line += 1;
            self.pos.column = 0;
        }
        else{
            self.pos.column += 1;
        }
        self.ch_0 = self.ch_1;
        self.ch_1 = self.chars.next().unwrap_or('\0');
        
        self.index += 1;
    }
    
    fn skip_two_chars(&mut self) {
        if self.ch_0 == '\n' || self.ch_1 == '\n'{
            panic!()
        }
        else{
            self.pos.column += 2;
        }
        self.ch_0 = self.chars.next().unwrap_or('\0');
        self.ch_1 = self.chars.next().unwrap_or('\0');
        self.index += 2;
    }
    
    fn begin_span(&mut self) -> SpanTracker {
        SpanTracker {
            file_id: self.file_id,
            pos: self.pos,
        }
    }
}

impl<C> Iterator for Lex<C>
where
C: Iterator<Item = char>,
{
    type Item = Result<TokenWithSpan, LiveError>;
    
    fn next(&mut self) -> Option<Result<TokenWithSpan, LiveError >> {
        if self.is_done {
            None
        } else {
            Some(self.read_token_with_span().map( | token_with_span | {
                if token_with_span.token == LiveToken::Eof {
                    self.is_done = true
                }
                token_with_span
            }))
        }
    }
}

pub struct LexResult {
    pub strings: Vec<char>,
    pub tokens: Vec<TokenWithSpan>
}

pub fn lex<C>(chars: C, start_pos:TextPos, file_id: LiveFileId) -> Result<LexResult, LiveError>
where
C: IntoIterator<Item = char>,
{
    let mut chars = chars.into_iter();
    let ch_0 = chars.next().unwrap_or('\0');
    let ch_1 = chars.next().unwrap_or('\0');
    let mut tokens = Vec::new();
    let mut lex = Lex {
        chars,
        ch_0,
        ch_1,
        file_id,
        index: 0,
        temp_hex: Vec::new(),
        temp_string: String::new(),
        group_stack: Vec::new(),
        strings: Vec::new(),
        pos: start_pos,
        is_done: false,
    };
    loop {
        match lex.read_token_with_span() {
            Err(err) => {
                return Err(err)
            },
            Ok(tok) => {
                tokens.push(tok);
                if tok.token == LiveToken::Eof {
                    break
                }
            }
        }
    }
    return Ok(LexResult {
        strings: lex.strings,
        tokens
    });
}

struct SpanTracker {
    file_id: LiveFileId,
    pos: TextPos
}

impl SpanTracker {
    fn token<C>(&self, lex: &Lex<C>, token: LiveToken) -> TokenWithSpan {
        TokenWithSpan {
            span: Span{
                file_id:self.file_id,
                start: self.pos,
                end: lex.pos
            },
            token,
        }
    }
    
    fn error<C>(&self, lex: &Lex<C>, message: String) -> LiveError {
        LiveError {
            origin: live_error_origin!(),
            span: Span{
                file_id:self.file_id,
                start: self.pos,
                end: lex.pos
            },
            message,
        }
    }
}
