# -*- coding: utf-8 -*-
# Generated by the protocol buffer compiler.  DO NOT EDIT!
# source: opcua_node.proto

from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from google.protobuf import reflection as _reflection
from google.protobuf import symbol_database as _symbol_database
# @@protoc_insertion_point(imports)

_sym_db = _symbol_database.Default()




DESCRIPTOR = _descriptor.FileDescriptor(
  name='opcua_node.proto',
  package='OpcuaNode',
  syntax='proto3',
  serialized_options=b'\252\002\tOpcuaNode',
  create_key=_descriptor._internal_create_key,
  serialized_pb=b'\n\x10opcua_node.proto\x12\tOpcuaNode\"\x0e\n\x0cValueRequest\"4\n\rValueResponse\x12\r\n\x05value\x18\x01 \x01(\x05\x12\x14\n\x0copcua_server\x18\x02 \x01(\t2J\n\tOpcuaNode\x12=\n\x08GetValue\x12\x17.OpcuaNode.ValueRequest\x1a\x18.OpcuaNode.ValueResponseB\x0c\xaa\x02\tOpcuaNodeb\x06proto3'
)




_VALUEREQUEST = _descriptor.Descriptor(
  name='ValueRequest',
  full_name='OpcuaNode.ValueRequest',
  filename=None,
  file=DESCRIPTOR,
  containing_type=None,
  create_key=_descriptor._internal_create_key,
  fields=[
  ],
  extensions=[
  ],
  nested_types=[],
  enum_types=[
  ],
  serialized_options=None,
  is_extendable=False,
  syntax='proto3',
  extension_ranges=[],
  oneofs=[
  ],
  serialized_start=31,
  serialized_end=45,
)


_VALUERESPONSE = _descriptor.Descriptor(
  name='ValueResponse',
  full_name='OpcuaNode.ValueResponse',
  filename=None,
  file=DESCRIPTOR,
  containing_type=None,
  create_key=_descriptor._internal_create_key,
  fields=[
    _descriptor.FieldDescriptor(
      name='value', full_name='OpcuaNode.ValueResponse.value', index=0,
      number=1, type=5, cpp_type=1, label=1,
      has_default_value=False, default_value=0,
      message_type=None, enum_type=None, containing_type=None,
      is_extension=False, extension_scope=None,
      serialized_options=None, file=DESCRIPTOR,  create_key=_descriptor._internal_create_key),
    _descriptor.FieldDescriptor(
      name='opcua_server', full_name='OpcuaNode.ValueResponse.opcua_server', index=1,
      number=2, type=9, cpp_type=9, label=1,
      has_default_value=False, default_value=b"".decode('utf-8'),
      message_type=None, enum_type=None, containing_type=None,
      is_extension=False, extension_scope=None,
      serialized_options=None, file=DESCRIPTOR,  create_key=_descriptor._internal_create_key),
  ],
  extensions=[
  ],
  nested_types=[],
  enum_types=[
  ],
  serialized_options=None,
  is_extendable=False,
  syntax='proto3',
  extension_ranges=[],
  oneofs=[
  ],
  serialized_start=47,
  serialized_end=99,
)

DESCRIPTOR.message_types_by_name['ValueRequest'] = _VALUEREQUEST
DESCRIPTOR.message_types_by_name['ValueResponse'] = _VALUERESPONSE
_sym_db.RegisterFileDescriptor(DESCRIPTOR)

ValueRequest = _reflection.GeneratedProtocolMessageType('ValueRequest', (_message.Message,), {
  'DESCRIPTOR' : _VALUEREQUEST,
  '__module__' : 'opcua_node_pb2'
  # @@protoc_insertion_point(class_scope:OpcuaNode.ValueRequest)
  })
_sym_db.RegisterMessage(ValueRequest)

ValueResponse = _reflection.GeneratedProtocolMessageType('ValueResponse', (_message.Message,), {
  'DESCRIPTOR' : _VALUERESPONSE,
  '__module__' : 'opcua_node_pb2'
  # @@protoc_insertion_point(class_scope:OpcuaNode.ValueResponse)
  })
_sym_db.RegisterMessage(ValueResponse)


DESCRIPTOR._options = None

_OPCUANODE = _descriptor.ServiceDescriptor(
  name='OpcuaNode',
  full_name='OpcuaNode.OpcuaNode',
  file=DESCRIPTOR,
  index=0,
  serialized_options=None,
  create_key=_descriptor._internal_create_key,
  serialized_start=101,
  serialized_end=175,
  methods=[
  _descriptor.MethodDescriptor(
    name='GetValue',
    full_name='OpcuaNode.OpcuaNode.GetValue',
    index=0,
    containing_service=None,
    input_type=_VALUEREQUEST,
    output_type=_VALUERESPONSE,
    serialized_options=None,
    create_key=_descriptor._internal_create_key,
  ),
])
_sym_db.RegisterServiceDescriptor(_OPCUANODE)

DESCRIPTOR.services_by_name['OpcuaNode'] = _OPCUANODE

# @@protoc_insertion_point(module_scope)
