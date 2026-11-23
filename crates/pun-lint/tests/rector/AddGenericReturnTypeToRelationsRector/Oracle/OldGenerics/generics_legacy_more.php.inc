<?php

namespace App;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\BelongsToMany;
use Illuminate\Database\Eloquent\Relations\HasManyThrough;
use Illuminate\Database\Eloquent\Relations\MorphTo;

class Account extends Model {}

class Car extends Model {}

class User extends Model
{
    /**
     * @return BelongsTo<Account>
     */
    public function account(): BelongsTo
    {
        return $this->belongsTo(Account::class);
    }

    /**
     * @return BelongsTo<Account, Car>
     */
    public function wrongChild(): BelongsTo
    {
        return $this->belongsTo(Account::class);
    }

    public function through(): HasManyThrough
    {
        return $this->hasManyThrough(Account::class, Car::class);
    }

    public function morph(): MorphTo
    {
        return $this->morphTo(Account::class);
    }

    public function many(): BelongsToMany
    {
        return $this->belongsToMany(Account::class);
    }
}
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\BelongsToMany;
use Illuminate\Database\Eloquent\Relations\HasManyThrough;
use Illuminate\Database\Eloquent\Relations\MorphTo;

class Account extends Model {}

class Car extends Model {}

class User extends Model
{
    /**
     * @return \Illuminate\Database\Eloquent\Relations\BelongsTo<\App\Account, \App\User>
     */
    public function account(): BelongsTo
    {
        return $this->belongsTo(Account::class);
    }

    /**
     * @return \Illuminate\Database\Eloquent\Relations\BelongsTo<\App\Account, \App\User>
     */
    public function wrongChild(): BelongsTo
    {
        return $this->belongsTo(Account::class);
    }

    public function through(): HasManyThrough
    {
        return $this->hasManyThrough(Account::class, Car::class);
    }

    /**
     * @return \Illuminate\Database\Eloquent\Relations\MorphTo<\App\Account, \App\User>
     */
    public function morph(): MorphTo
    {
        return $this->morphTo(Account::class);
    }

    /**
     * @return \Illuminate\Database\Eloquent\Relations\BelongsToMany<\App\Account>
     */
    public function many(): BelongsToMany
    {
        return $this->belongsToMany(Account::class);
    }
}
