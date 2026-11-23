<?php

namespace App;

use Illuminate\Foundation\Http\FormRequest;

class Upper extends FormRequest
{
    public function authorize(): bool
    {
        return TRUE;
    }
}

class Wrapped extends FormRequest
{
    public function authorize()
    {
        return (true);
    }

    public function rules(): array
    {
        return [];
    }
}

class WithParam extends FormRequest
{
    public function authorize($user = null): bool
    {
        return true;
    }
}

class Qualified extends FormRequest
{
    public function authorize(): bool
    {
        return \true;
    }
}

$request = new class extends FormRequest {
    public function authorize(): bool
    {
        return true;
    }
};
-----
<?php

namespace App;

use Illuminate\Foundation\Http\FormRequest;

class Upper extends FormRequest
{
}

class Wrapped extends FormRequest
{
    public function rules(): array
    {
        return [];
    }
}

class WithParam extends FormRequest
{
    public function authorize($user = null): bool
    {
        return true;
    }
}

class Qualified extends FormRequest
{
}

$request = new class extends FormRequest
{
};
