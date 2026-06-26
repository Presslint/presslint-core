#![allow(clippy::expect_used, clippy::missing_errors_doc)]

use std::{fmt, vec::IntoIter};

use serde::{
    Deserialize, Serialize, de, forward_to_deserialize_any,
    ser::{self, SerializeSeq, SerializeStruct},
};

use super::{
    NamedOutputCondition, OutputIntentPolicy, OutputIntentSubtype, OutputIntentTarget,
    OutputProfileSource, ProfileBackedOutputIntent,
};

fn assert_json_round_trip<T>(value: &T, expected: Json)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + fmt::Debug,
{
    let encoded = value.serialize(JsonSerializer).expect("serialize value");
    assert_eq!(encoded, expected);

    let decoded = T::deserialize(expected).expect("deserialize fixture");
    assert_eq!(&decoded, value);
}

#[test]
fn output_intent_policy_variants_have_stable_json_shape() {
    assert_json_round_trip(
        &OutputIntentPolicy::Preserve,
        Json::object([("policy", Json::string("preserve"))]),
    );
    assert_json_round_trip(
        &OutputIntentPolicy::RequireExisting,
        Json::object([("policy", Json::string("require_existing"))]),
    );
    assert_json_round_trip(
        &OutputIntentPolicy::EnsureTarget {
            target: OutputIntentTarget::NamedCondition {
                condition: named_condition(),
            },
        },
        Json::object([
            ("policy", Json::string("ensure_target")),
            (
                "target",
                Json::object([
                    ("kind", Json::string("named_condition")),
                    ("condition", named_condition_json()),
                ]),
            ),
        ]),
    );
}

#[test]
fn output_intent_target_variants_have_stable_json_shape() {
    assert_json_round_trip(
        &OutputIntentTarget::NamedCondition {
            condition: named_condition(),
        },
        Json::object([
            ("kind", Json::string("named_condition")),
            ("condition", named_condition_json()),
        ]),
    );
    assert_json_round_trip(
        &OutputIntentTarget::ProfileBacked {
            intent: profile_backed_intent(),
        },
        Json::object([
            ("kind", Json::string("profile_backed")),
            ("intent", profile_backed_intent_json()),
        ]),
    );
}

#[test]
fn output_intent_subtype_has_stable_json_shape() {
    assert_json_round_trip(&OutputIntentSubtype::GtsPdfx, Json::string("gts_pdfx"));
    assert_json_round_trip(&OutputIntentSubtype::GtsPdfa1, Json::string("gts_pdfa1"));
    assert_json_round_trip(&OutputIntentSubtype::IsoPdfe1, Json::string("iso_pdfe1"));
}

#[test]
fn named_output_condition_has_stable_json_shape() {
    assert_json_round_trip(&named_condition(), named_condition_json());
}

#[test]
fn profile_backed_output_intent_has_stable_json_shape() {
    assert_json_round_trip(&profile_backed_intent(), profile_backed_intent_json());
}

#[test]
fn output_profile_source_variants_have_stable_json_shape() {
    assert_json_round_trip(
        &OutputProfileSource::OpaqueId {
            id: "profile:pso-coated-v3".to_owned(),
        },
        Json::object([
            ("source", Json::string("opaque_id")),
            ("id", Json::string("profile:pso-coated-v3")),
        ]),
    );
    assert_json_round_trip(
        &OutputProfileSource::EmbeddedBytes {
            bytes: vec![0, 1, 2, 255],
        },
        Json::object([
            ("source", Json::string("embedded_bytes")),
            (
                "bytes",
                Json::array([Json::U32(0), Json::U32(1), Json::U32(2), Json::U32(255)]),
            ),
        ]),
    );
}

fn named_condition() -> NamedOutputCondition {
    NamedOutputCondition {
        subtype: OutputIntentSubtype::GtsPdfx,
        output_condition_identifier: "FOGRA51".to_owned(),
        registry_name: "http://www.color.org".to_owned(),
    }
}

fn named_condition_json() -> Json {
    Json::object([
        ("subtype", Json::string("gts_pdfx")),
        ("output_condition_identifier", Json::string("FOGRA51")),
        ("registry_name", Json::string("http://www.color.org")),
    ])
}

fn profile_backed_intent() -> ProfileBackedOutputIntent {
    ProfileBackedOutputIntent {
        subtype: OutputIntentSubtype::GtsPdfx,
        output_condition_identifier: "Custom".to_owned(),
        output_condition: "Coated".to_owned(),
        info: "Coated 150lpi".to_owned(),
        profile: OutputProfileSource::OpaqueId {
            id: "profiles/coated.icc".to_owned(),
        },
    }
}

fn profile_backed_intent_json() -> Json {
    Json::object([
        ("subtype", Json::string("gts_pdfx")),
        ("output_condition_identifier", Json::string("Custom")),
        ("output_condition", Json::string("Coated")),
        ("info", Json::string("Coated 150lpi")),
        (
            "profile",
            Json::object([
                ("source", Json::string("opaque_id")),
                ("id", Json::string("profiles/coated.icc")),
            ]),
        ),
    ])
}

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Object(Vec<(String, Self)>),
    Array(Vec<Self>),
    String(String),
    U32(u32),
}

impl Json {
    fn object(fields: impl IntoIterator<Item = (&'static str, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn array(values: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(values.into_iter().collect())
    }

    fn string(value: &'static str) -> Self {
        Self::String(value.to_owned())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct JsonError(String);

impl JsonError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for JsonError {}

impl ser::Error for JsonError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl de::Error for JsonError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

struct JsonSerializer;

impl ser::Serializer for JsonSerializer {
    type Ok = Json;
    type Error = JsonError;
    type SerializeSeq = JsonArraySerializer;
    type SerializeTuple = JsonArraySerializer;
    type SerializeTupleStruct = JsonArraySerializer;
    type SerializeTupleVariant = JsonArraySerializer;
    type SerializeMap = JsonObjectSerializer;
    type SerializeStruct = JsonObjectSerializer;
    type SerializeStructVariant = JsonObjectSerializer;

    fn serialize_bool(self, _value: bool) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("unsupported bool JSON value"))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        let value = u32::try_from(value).map_err(Self::Error::custom)?;
        Ok(Json::U32(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.serialize_u32(u32::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_u32(u32::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(Json::U32(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        let value = u32::try_from(value).map_err(Self::Error::custom)?;
        Ok(Json::U32(value))
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("unsupported float JSON value"))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("unsupported float JSON value"))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(&value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Json::String(value.to_owned()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(Json::Array(
            value
                .iter()
                .map(|byte| Json::U32(u32::from(*byte)))
                .collect(),
        ))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("unsupported null JSON value"))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("unsupported unit JSON value"))
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom(format!(
            "unsupported unit struct {name}"
        )))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(Json::String(variant.to_owned()))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(Json::object([(variant, value.serialize(Self)?)]))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(JsonArraySerializer { values: Vec::new() })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(JsonObjectSerializer {
            fields: Vec::new(),
            next_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(None)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.serialize_map(None)
    }
}

struct JsonArraySerializer {
    values: Vec<Json>,
}

impl SerializeSeq for JsonArraySerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.values.push(value.serialize(JsonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Json::Array(self.values))
    }
}

impl ser::SerializeTuple for JsonArraySerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for JsonArraySerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for JsonArraySerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

struct JsonObjectSerializer {
    fields: Vec<(String, Json)>,
    next_key: Option<String>,
}

impl ser::SerializeMap for JsonObjectSerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Self::Error> {
        match key.serialize(JsonSerializer)? {
            Json::String(key) => {
                self.next_key = Some(key);
                Ok(())
            }
            other => Err(Self::Error::custom(format!(
                "unsupported object key {other:?}"
            ))),
        }
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| Self::Error::custom("missing object key"))?;
        self.fields.push((key, value.serialize(JsonSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Json::Object(self.fields))
    }
}

impl SerializeStruct for JsonObjectSerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.fields
            .push((key.to_owned(), value.serialize(JsonSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Json::Object(self.fields))
    }
}

impl ser::SerializeStructVariant for JsonObjectSerializer {
    type Ok = Json;
    type Error = JsonError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeStruct::end(self)
    }
}

impl de::IntoDeserializer<'_, JsonError> for Json {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> de::Deserializer<'de> for Json {
    type Error = JsonError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            Self::Object(fields) => visitor.visit_map(JsonMapAccess {
                fields: fields.into_iter(),
                next_value: None,
            }),
            Self::Array(values) => visitor.visit_seq(JsonSeqAccess {
                values: values.into_iter(),
            }),
            Self::String(value) => visitor.visit_string(value),
            Self::U32(value) => visitor.visit_u32(value),
        }
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self {
            Self::String(variant) => visitor.visit_enum(JsonEnumAccess { variant }),
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

struct JsonMapAccess {
    fields: IntoIter<(String, Json)>,
    next_value: Option<Json>,
}

impl<'de> de::MapAccess<'de> for JsonMapAccess {
    type Error = JsonError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        let Some((key, value)) = self.fields.next() else {
            return Ok(None);
        };
        self.next_value = Some(value);
        seed.deserialize(Json::String(key)).map(Some)
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let value = self
            .next_value
            .take()
            .ok_or_else(|| Self::Error::custom("missing object value"))?;
        seed.deserialize(value)
    }
}

struct JsonSeqAccess {
    values: IntoIter<Json>,
}

impl<'de> de::SeqAccess<'de> for JsonSeqAccess {
    type Error = JsonError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        self.values
            .next()
            .map(|value| seed.deserialize(value))
            .transpose()
    }
}

struct JsonEnumAccess {
    variant: String,
}

impl<'de> de::EnumAccess<'de> for JsonEnumAccess {
    type Error = JsonError;
    type Variant = JsonVariantAccess;

    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = seed.deserialize(Json::String(self.variant))?;
        Ok((variant, JsonVariantAccess))
    }
}

struct JsonVariantAccess;

impl<'de> de::VariantAccess<'de> for JsonVariantAccess {
    type Error = JsonError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(
        self,
        _seed: T,
    ) -> Result<T::Value, Self::Error> {
        Err(Self::Error::custom("unsupported newtype enum variant"))
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(Self::Error::custom("unsupported tuple enum variant"))
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(Self::Error::custom("unsupported struct enum variant"))
    }
}
