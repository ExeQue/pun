<?php

class Shapes
{
    public $value;

    public function run(\stdClass $object, array $array, ?\stdClass $maybe, self $self, \stdClass|\ArrayObject $either)
    {
        array_key_exists('value', $object);
        array_key_exists('value', $array);
        array_key_exists('value', $maybe);
        array_key_exists('value', $self);
        array_key_exists('value', $either);
        array_key_exists('value', new \stdClass());
        array_key_exists(key: 'value', array: $object);
    }
}
-----
<?php

class Shapes
{
    public $value;

    public function run(\stdClass $object, array $array, ?\stdClass $maybe, self $self, \stdClass|\ArrayObject $either)
    {
        property_exists($object, 'value');
        array_key_exists('value', $array);
        array_key_exists('value', $maybe);
        property_exists($self, 'value');
        array_key_exists('value', $either);
        property_exists(new \stdClass(), 'value');
        property_exists(array: $object, key: 'value');
    }
}
