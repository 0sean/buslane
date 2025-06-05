class AddShapeToTrip < ActiveRecord::Migration[8.0]
  def change
    add_reference :trips, :shape, null: true, type: :string
  end
end
