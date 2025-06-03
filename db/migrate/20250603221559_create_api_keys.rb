class CreateApiKeys < ActiveRecord::Migration[8.0]
  def change
    create_table :api_keys do |t|
      t.string :name
      t.text :token_digest

      t.timestamps
    end
    add_index :api_keys, :token_digest, unique: true
  end
end
