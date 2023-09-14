mod attr;
mod mp4_prim;
mod mp4_value;
mod parse_box;
mod parse_boxes;
mod parsed_box;
mod util;

use synstructure::decl_derive;

decl_derive!([ParseBox, attributes(box_type)] => parse_box::derive);
decl_derive!([ParsedBox, attributes(box_type)] => parsed_box::derive);
decl_derive!([ParseBoxes, attributes(box_type)] => parse_boxes::derive);
decl_derive!([Mp4Prim] => mp4_prim::derive);
decl_derive!([Mp4Value] => mp4_value::derive);
