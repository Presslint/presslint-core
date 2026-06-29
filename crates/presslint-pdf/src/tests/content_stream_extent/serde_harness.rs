use serde::de::IntoDeserializer;

#[derive(Debug)]
pub struct TestSerdeError(String);

impl std::fmt::Display for TestSerdeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TestSerdeError {}

impl serde::ser::Error for TestSerdeError {
    fn custom<T>(message: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self(message.to_string())
    }
}

impl serde::de::Error for TestSerdeError {
    fn custom<T>(message: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self(message.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestSerdeValue {
    Bool(bool),
    U64(u64),
    String(String),
    None,
    Some(Box<Self>),
    Seq(Vec<Self>),
    Map(Vec<(String, Self)>),
    Unit,
}

pub fn serde_value<T>(value: &T) -> Result<TestSerdeValue, TestSerdeError>
where
    T: serde::Serialize,
{
    value.serialize(TestSerdeSerializer)
}

pub fn from_serde_value<T>(value: TestSerdeValue) -> Result<T, TestSerdeError>
where
    T: serde::de::DeserializeOwned,
{
    T::deserialize(value)
}

struct TestSerdeSerializer;

impl serde::Serializer for TestSerdeSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;
    type SerializeSeq = TestSerdeSeqSerializer;
    type SerializeTuple = TestSerdeSeqSerializer;
    type SerializeTupleStruct = TestSerdeSeqSerializer;
    type SerializeTupleVariant = TestSerdeSeqSerializer;
    type SerializeMap = TestSerdeMapSerializer;
    type SerializeStruct = TestSerdeMapSerializer;
    type SerializeStructVariant = TestSerdeMapSerializer;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::Bool(value))
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
        let value = u64::try_from(value).map_err(serde::ser::Error::custom)?;
        Ok(TestSerdeValue::U64(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::U64(value))
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("f32 is unsupported"))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("f64 is unsupported"))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(&value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::String(value.to_string()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        let values = value
            .iter()
            .copied()
            .map(u64::from)
            .map(TestSerdeValue::U64)
            .collect();
        Ok(TestSerdeValue::Seq(values))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::None)
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        Ok(TestSerdeValue::Some(Box::new(value.serialize(self)?)))
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::Unit)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        Ok(TestSerdeValue::Map(vec![(
            variant.to_string(),
            value.serialize(self)?,
        )]))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TestSerdeSeqSerializer { values: Vec::new() })
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
        Ok(TestSerdeMapSerializer {
            values: Vec::new(),
            next_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut serializer = self.serialize_map(Some(len + 1))?;
        serializer.values.push((
            "variant".to_string(),
            TestSerdeValue::String(variant.to_string()),
        ));
        Ok(serializer)
    }
}

struct TestSerdeSeqSerializer {
    values: Vec<TestSerdeValue>,
}

impl serde::ser::SerializeSeq for TestSerdeSeqSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        self.values.push(value.serialize(TestSerdeSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::Seq(self.values))
    }
}

impl serde::ser::SerializeTuple for TestSerdeSeqSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleStruct for TestSerdeSeqSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleVariant for TestSerdeSeqSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

struct TestSerdeMapSerializer {
    values: Vec<(String, TestSerdeValue)>,
    next_key: Option<String>,
}

impl serde::ser::SerializeMap for TestSerdeMapSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        match key.serialize(TestSerdeSerializer)? {
            TestSerdeValue::String(key) => {
                self.next_key = Some(key);
                Ok(())
            }
            _ => Err(serde::ser::Error::custom("map key must be a string")),
        }
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| serde::ser::Error::custom("map value without key"))?;
        self.values
            .push((key, value.serialize(TestSerdeSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(TestSerdeValue::Map(self.values))
    }
}

impl serde::ser::SerializeStruct for TestSerdeMapSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        self.values
            .push((key.to_string(), value.serialize(TestSerdeSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeMap::end(self)
    }
}

impl serde::ser::SerializeStructVariant for TestSerdeMapSerializer {
    type Ok = TestSerdeValue;
    type Error = TestSerdeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeMap::end(self)
    }
}

impl<'de> serde::Deserializer<'de> for TestSerdeValue {
    type Error = TestSerdeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Bool(value) => visitor.visit_bool(value),
            Self::U64(value) => visitor.visit_u64(value),
            Self::String(value) => visitor.visit_string(value),
            Self::None | Self::Unit => visitor.visit_unit(),
            Self::Some(value) => value.deserialize_any(visitor),
            Self::Seq(values) => visitor.visit_seq(TestSerdeSeqAccess {
                values: values.into_iter(),
            }),
            Self::Map(values) => visitor.visit_map(TestSerdeMapAccess {
                values: values.into_iter(),
                next_value: None,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Bool(value) => visitor.visit_bool(value),
            _ => Err(serde::de::Error::custom("expected bool")),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::U64(value) => {
                let value = i64::try_from(value).map_err(serde::de::Error::custom)?;
                visitor.visit_i64(value)
            }
            _ => Err(serde::de::Error::custom("expected integer")),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::U64(value) => visitor.visit_u64(value),
            _ => Err(serde::de::Error::custom("expected unsigned integer")),
        }
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(serde::de::Error::custom("f32 is unsupported"))
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(serde::de::Error::custom("f64 is unsupported"))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::String(value) => {
                let mut chars = value.chars();
                let Some(value) = chars.next() else {
                    return Err(serde::de::Error::custom("expected char"));
                };
                if chars.next().is_some() {
                    return Err(serde::de::Error::custom("expected char"));
                }
                visitor.visit_char(value)
            }
            _ => Err(serde::de::Error::custom("expected char")),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_string(value),
            _ => Err(serde::de::Error::custom("expected string")),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::None => visitor.visit_none(),
            Self::Some(value) => visitor.visit_some(*value),
            value => visitor.visit_some(value),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Unit | Self::None => visitor.visit_unit(),
            _ => Err(serde::de::Error::custom("expected unit")),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Seq(values) => visitor.visit_seq(TestSerdeSeqAccess {
                values: values.into_iter(),
            }),
            _ => Err(serde::de::Error::custom("expected sequence")),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Map(values) => visitor.visit_map(TestSerdeMapAccess {
                values: values.into_iter(),
                next_value: None,
            }),
            _ => Err(serde::de::Error::custom("expected map")),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_enum(value.into_deserializer()),
            Self::Map(values) => visitor.visit_map(TestSerdeMapAccess {
                values: values.into_iter(),
                next_value: None,
            }),
            _ => Err(serde::de::Error::custom("expected enum")),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct TestSerdeSeqAccess {
    values: std::vec::IntoIter<TestSerdeValue>,
}

impl<'de> serde::de::SeqAccess<'de> for TestSerdeSeqAccess {
    type Error = TestSerdeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        self.values
            .next()
            .map(|value| seed.deserialize(value))
            .transpose()
    }
}

struct TestSerdeMapAccess {
    values: std::vec::IntoIter<(String, TestSerdeValue)>,
    next_value: Option<TestSerdeValue>,
}

impl<'de> serde::de::MapAccess<'de> for TestSerdeMapAccess {
    type Error = TestSerdeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.values.next() else {
            return Ok(None);
        };
        self.next_value = Some(value);
        seed.deserialize(TestSerdeValue::String(key)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        let value = self
            .next_value
            .take()
            .ok_or_else(|| serde::de::Error::custom("map value without key"))?;
        seed.deserialize(value)
    }
}
